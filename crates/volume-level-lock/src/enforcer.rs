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

pub enum EnforcerEvent {
  RebindRole(AudioFlow, ERole),
  VolumeFileChanged,
}

const ROLE_SLOTS: usize = 3;
const VOLUME_EPSILON: f32 = 0.005;

fn role_slot(role: ERole) -> usize {
  match role.0 {
    v if v == eConsole.0 => 0,
    v if v == eMultimedia.0 => 1,
    v if v == eCommunications.0 => 2,
    _ => 0,
  }
}

#[derive(Default)]
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

pub struct AudioEnforcer {
  flow: AudioFlow,
  state: Arc<VolumeState>,
  enumerator: IMMDeviceEnumerator,
  notification_client: Option<IMMNotificationClient>,
  bindings: [Option<AudioBinding>; ROLE_SLOTS],
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
    unsafe {
      let _ = self.endpoint.UnregisterControlChangeNotify(&self.callback);
    }
  }
}

impl Drop for AudioEnforcer {
  fn drop(&mut self) {
    let _ = self.disable();
  }
}

impl AudioEnforcer {
  pub fn new(
    flow: AudioFlow,
    state: Arc<VolumeState>,
    event_tx: Sender<EnforcerEvent>,
    main_thread_id: u32,
  ) -> Result<Self> {
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

  pub fn enable(&mut self) -> Result<()> {
    if self.enabled {
      return Ok(());
    }

    // Setup device notification client to detect default device changes
    let client = DeviceNotificationClient::new(
      self.flow,
      self.event_tx.clone(),
      self.main_thread_id,
    );
    let client_interface: IMMNotificationClient = client.into();
    unsafe {
      self
        .enumerator
        .RegisterEndpointNotificationCallback(&client_interface)?;
    }
    self.notification_client = Some(client_interface);

    // Bind existing active endpoints
    for role in &[eConsole, eMultimedia, eCommunications] {
      let _ = self.bind_role(*role);
    }

    self.enabled = true;
    Ok(())
  }

  pub fn disable(&mut self) -> Result<()> {
    if !self.enabled {
      return Ok(());
    }

    // Unregister notifications
    if let Some(ref client) = self.notification_client {
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
    Ok(())
  }

  fn flow_to_win_flow(flow: AudioFlow) -> EDataFlow {
    match flow {
      AudioFlow::Input => eCapture,
      AudioFlow::Output => eRender,
    }
  }

  pub fn bind_role(&mut self, role: ERole) -> Result<()> {
    let slot = role_slot(role);
    // Drop the existing binding for this slot, if any (triggers Drop,
    // which unregisters its notify callback).
    self.bindings[slot] = None;

    unsafe {
      let win_flow = Self::flow_to_win_flow(self.flow);
      let default_device =
        self.enumerator.GetDefaultAudioEndpoint(win_flow, role)?;

      // Let's activate using standard COM interface retrieval
      let endpoint_volume_obj: IAudioEndpointVolume =
        default_device.Activate(CLSCTX_INPROC_SERVER, None)?;

      let callback = VolumeNotificationCallback::new(
        endpoint_volume_obj.clone(),
        self.state.clone(),
        self.flow,
        self.context_guid,
      );
      let callback_interface: IAudioEndpointVolumeCallback = callback.into();

      endpoint_volume_obj.RegisterControlChangeNotify(&callback_interface)?;

      self.bindings[slot] = Some(AudioBinding {
        endpoint: endpoint_volume_obj,
        callback: callback_interface,
      });
    }

    Ok(())
  }

  pub fn force_to_target(&self) {
    let val = self.state.scalar(self.flow);
    for binding in self.bindings.iter().flatten() {
      unsafe {
        let needs_set = match binding.endpoint.GetMasterVolumeLevelScalar() {
          Ok(current) => (current - val).abs() > VOLUME_EPSILON,
          Err(_) => true,
        };
        if needs_set {
          let _ = binding
            .endpoint
            .SetMasterVolumeLevelScalar(val, &self.context_guid);
        }
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

impl VolumeNotificationCallback {
  fn new(
    endpoint: IAudioEndpointVolume,
    state: Arc<VolumeState>,
    flow: AudioFlow,
    context_guid: GUID,
  ) -> Self {
    Self {
      endpoint,
      state,
      flow,
      context_guid,
    }
  }
}

impl IAudioEndpointVolumeCallback_Impl for VolumeNotificationCallback_Impl {
  fn OnNotify(
    &self,
    notification_data_ptr: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
  ) -> WinResult<()> {
    if notification_data_ptr.is_null() {
      return Ok(());
    }
    let data = unsafe { &*notification_data_ptr };
    // Ignore changes triggered by ourselves
    if data.guidEventContext == self.context_guid {
      return Ok(());
    }

    let target_val = self.state.scalar(self.flow);
    if (data.fMasterVolume - target_val).abs() > VOLUME_EPSILON {
      unsafe {
        let _ = self
          .endpoint
          .SetMasterVolumeLevelScalar(target_val, &self.context_guid);
      }
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

impl DeviceNotificationClient {
  fn new(
    flow: AudioFlow,
    event_tx: Sender<EnforcerEvent>,
    main_thread_id: u32,
  ) -> Self {
    Self {
      flow,
      event_tx,
      main_thread_id,
    }
  }
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
    let target_flow = AudioEnforcer::flow_to_win_flow(self.flow);
    if flow == target_flow {
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
