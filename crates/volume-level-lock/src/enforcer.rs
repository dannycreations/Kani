#![cfg(windows)]

use std::sync::{
  atomic::{AtomicBool, AtomicU32, Ordering},
  mpsc::Sender,
  Arc,
};

use anyhow::Result;
use windows::{
  core::{implement, Result as WinResult, GUID, PCWSTR},
  Win32::{
    Foundation::PROPERTYKEY,
    Media::Audio::{
      eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow,
      ERole,
      Endpoints::{
        IAudioEndpointVolume, IAudioEndpointVolumeCallback,
        IAudioEndpointVolumeCallback_Impl,
      },
      IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
      MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
    },
    System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
  },
};

use crate::utils::wake_main_thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFlow {
  Input,
  Output,
}

impl AudioFlow {
  fn data_flow(self) -> EDataFlow {
    match self {
      Self::Input => eCapture,
      Self::Output => eRender,
    }
  }
}

pub enum EnforcerEvent {
  RebindRole(AudioFlow, ERole),
  VolumeFileChanged,
}

const ROLES: [ERole; 3] = [eConsole, eMultimedia, eCommunications];
const VOLUME_EPSILON: f32 = 0.005;

fn role_slot(role: ERole) -> usize {
  ROLES.iter().position(|&known| known == role).unwrap_or(0)
}

pub struct VolumeState {
  pub input_target: AtomicU32,
  pub output_target: AtomicU32,
  pub input_paused: AtomicBool,
  pub output_paused: AtomicBool,
}

impl VolumeState {
  pub fn new(
    input_target: u32,
    output_target: u32,
    input_paused: bool,
    output_paused: bool,
  ) -> Self {
    Self {
      input_target: AtomicU32::new(input_target),
      output_target: AtomicU32::new(output_target),
      input_paused: AtomicBool::new(input_paused),
      output_paused: AtomicBool::new(output_paused),
    }
  }

  #[inline]
  pub fn target(&self, flow: AudioFlow) -> &AtomicU32 {
    match flow {
      AudioFlow::Input => &self.input_target,
      AudioFlow::Output => &self.output_target,
    }
  }

  #[inline]
  pub fn paused(&self, flow: AudioFlow) -> &AtomicBool {
    match flow {
      AudioFlow::Input => &self.input_paused,
      AudioFlow::Output => &self.output_paused,
    }
  }

  #[inline]
  pub fn scalar(&self, flow: AudioFlow) -> f32 {
    (self.target(flow).load(Ordering::SeqCst) as f32 / 100.0).clamp(0.0, 1.0)
  }
}

fn is_drifted(current: f32, target: f32) -> bool {
  (current - target).abs() > VOLUME_EPSILON
}

fn set_volume(
  endpoint: &IAudioEndpointVolume,
  level: f32,
  context_guid: &GUID,
) {
  // SAFETY: `endpoint` is a live interface owned by the caller, and
  // `context_guid` only tags the change so our own notification callback
  // can recognise and ignore it. Both stay alive across the call.
  unsafe {
    let _ = endpoint.SetMasterVolumeLevelScalar(level, context_guid);
  }
}

pub struct AudioEnforcer {
  flow: AudioFlow,
  state: Arc<VolumeState>,
  enumerator: IMMDeviceEnumerator,
  notification_client: Option<IMMNotificationClient>,
  bindings: [Option<AudioBinding>; ROLES.len()],
  enabled: bool,
  context_guid: GUID,
  event_tx: Sender<EnforcerEvent>,
  main_thread_id: u32,
}

struct AudioBinding {
  endpoint: IAudioEndpointVolume,
  callback: IAudioEndpointVolumeCallback,
}

impl Drop for AudioBinding {
  fn drop(&mut self) {
    // SAFETY: `self.endpoint` is the live interface this binding holds,
    // and `self.callback` is the interface it was registered with, so
    // this only unregisters that pair. Both are alive here and the
    // endpoint refuses the call once it is shutting down.
    unsafe {
      let _ = self.endpoint.UnregisterControlChangeNotify(&self.callback);
    }
  }
}

impl Drop for AudioEnforcer {
  fn drop(&mut self) {
    self.disable();
  }
}

impl AudioEnforcer {
  pub fn new(
    flow: AudioFlow,
    state: Arc<VolumeState>,
    event_tx: Sender<EnforcerEvent>,
    main_thread_id: u32,
  ) -> Result<Self> {
    // SAFETY: `MMDeviceEnumerator` is a registered in-process COM class
    // and needs no outer IUnknown, so `None` is correct here.
    let enumerator: IMMDeviceEnumerator = unsafe {
      CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?
    };
    let context_guid = GUID::new()?;
    Ok(Self {
      flow,
      state,
      enumerator,
      notification_client: None,
      bindings: Default::default(),
      enabled: false,
      context_guid,
      event_tx,
      main_thread_id,
    })
  }

  pub fn flow(&self) -> AudioFlow {
    self.flow
  }

  pub fn enable(&mut self) -> Result<()> {
    if self.enabled {
      return Ok(());
    }

    // Setup device notification client to detect default device changes
    let client_interface: IMMNotificationClient = DeviceNotificationClient {
      flow: self.flow,
      event_tx: self.event_tx.clone(),
      main_thread_id: self.main_thread_id,
    }
    .into();
    // SAFETY: `client_interface` is a live COM interface, and
    // `self.enumerator` is the enumerator that owns the registration
    // list. The field keeps the client alive until `disable` unregisters
    // it, which is what keeps the callback from outliving this struct.
    unsafe {
      self
        .enumerator
        .RegisterEndpointNotificationCallback(&client_interface)?;
    }
    self.notification_client = Some(client_interface);

    // Bind existing active endpoints
    for &role in &ROLES {
      let _ = self.bind_role(role);
    }

    self.enabled = true;
    Ok(())
  }

  pub fn disable(&mut self) {
    if !self.enabled {
      return;
    }

    // Unregister notifications
    if let Some(ref client) = self.notification_client {
      // SAFETY: `client` is the exact interface registered in `enable`
      // and `self.enumerator` is the same enumerator, so this only
      // reverses that registration.
      unsafe {
        let _ = self
          .enumerator
          .UnregisterEndpointNotificationCallback(client);
      }
    }
    self.notification_client = None;

    // Drop all bindings (unregisters each endpoint's notify callback).
    self.bindings = Default::default();

    self.enabled = false;
  }

  pub fn bind_role(&mut self, role: ERole) -> Result<()> {
    let slot = role_slot(role);
    // Drop the existing binding for this slot, if any (triggers Drop,
    // which unregisters its notify callback).
    self.bindings[slot] = None;

    // SAFETY: the enumerator and the returned default device are live
    // COM interfaces, and the device only hands out an activated volume
    // interface that this function immediately registers and keeps
    // alive through the stored binding.
    unsafe {
      let default_device = self
        .enumerator
        .GetDefaultAudioEndpoint(self.flow.data_flow(), role)?;
      let endpoint: IAudioEndpointVolume =
        default_device.Activate(CLSCTX_INPROC_SERVER, None)?;

      let callback: IAudioEndpointVolumeCallback = VolumeNotificationCallback {
        endpoint: endpoint.clone(),
        state: self.state.clone(),
        flow: self.flow,
        context_guid: self.context_guid,
      }
      .into();
      endpoint.RegisterControlChangeNotify(&callback)?;

      self.bindings[slot] = Some(AudioBinding { endpoint, callback });
    }

    Ok(())
  }

  pub fn force_to_target(&self) {
    let target = self.state.scalar(self.flow);
    for binding in self.bindings.iter().flatten() {
      // SAFETY: `binding.endpoint` is a live interface owned by the
      // binding and this getter only reads from it.
      let needs_set =
        match unsafe { binding.endpoint.GetMasterVolumeLevelScalar() } {
          Ok(current) => is_drifted(current, target),
          // A level that cannot be read counts as drift, so the write
          // below re-establishes the target.
          Err(_) => true,
        };
      if needs_set {
        set_volume(&binding.endpoint, target, &self.context_guid);
      }
    }
  }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeNotificationCallback {
  endpoint: IAudioEndpointVolume,
  state: Arc<VolumeState>,
  flow: AudioFlow,
  context_guid: GUID,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeNotificationCallback_Impl {
  fn OnNotify(
    &self,
    notification_data_ptr: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
  ) -> WinResult<()> {
    if notification_data_ptr.is_null() {
      return Ok(());
    }
    // SAFETY: the null check above rules out a null pointer, and the
    // COM contract documents this callback's parameter as a pointer to a
    // live `AUDIO_VOLUME_NOTIFICATION_DATA` valid for the call.
    let data = unsafe { &*notification_data_ptr };
    // Ignore changes triggered by ourselves
    if data.guidEventContext == self.context_guid {
      return Ok(());
    }

    let target = self.state.scalar(self.flow);
    if is_drifted(data.fMasterVolume, target) {
      set_volume(&self.endpoint, target, &self.context_guid);
    }

    Ok(())
  }
}

#[implement(IMMNotificationClient)]
struct DeviceNotificationClient {
  flow: AudioFlow,
  event_tx: Sender<EnforcerEvent>,
  main_thread_id: u32,
}

impl IMMNotificationClient_Impl for DeviceNotificationClient_Impl {
  fn OnDeviceStateChanged(
    &self,
    _device_id_ptr: &PCWSTR,
    _new_state: DEVICE_STATE,
  ) -> WinResult<()> {
    Ok(())
  }

  fn OnDeviceAdded(&self, _device_id_ptr: &PCWSTR) -> WinResult<()> {
    Ok(())
  }

  fn OnDeviceRemoved(&self, _device_id_ptr: &PCWSTR) -> WinResult<()> {
    Ok(())
  }

  fn OnDefaultDeviceChanged(
    &self,
    flow: EDataFlow,
    role: ERole,
    _default_device_id_ptr: &PCWSTR,
  ) -> WinResult<()> {
    if flow == self.flow.data_flow() {
      let _ = self
        .event_tx
        .send(EnforcerEvent::RebindRole(self.flow, role));
      wake_main_thread(self.main_thread_id);
    }
    Ok(())
  }

  fn OnPropertyValueChanged(
    &self,
    _device_id_ptr: &PCWSTR,
    _key: &PROPERTYKEY,
  ) -> WinResult<()> {
    Ok(())
  }
}
