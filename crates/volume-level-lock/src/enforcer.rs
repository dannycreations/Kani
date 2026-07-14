#![cfg(windows)]

use std::sync::{
  atomic::{AtomicU32, Ordering},
  mpsc::Sender,
  Arc,
};

use anyhow::Result;
use windows::Win32::{
  Foundation::{LPARAM, PROPERTYKEY, WPARAM},
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
  UI::WindowsAndMessaging::PostThreadMessageW,
};
use windows_core::{implement, Result as WinResult, GUID, PCWSTR};

use crate::WM_WAKEUP;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFlow {
  Input,
  Output,
}

pub enum EnforcerEvent {
  RebindRole(AudioFlow, ERole),
  VolumeFileChanged,
}

pub struct AudioEnforcer {
  flow: AudioFlow,
  target: Arc<AtomicU32>,
  enumerator: IMMDeviceEnumerator,
  notification_client: Option<IMMNotificationClient>,
  bindings_and_roles: Vec<(ERole, AudioBinding)>,
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
    target: Arc<AtomicU32>,
    event_tx: Sender<EnforcerEvent>,
    main_thread_id: u32,
  ) -> Result<Self> {
    let enumerator: IMMDeviceEnumerator = unsafe {
      CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?
    };
    let context_guid = GUID::new()?;
    Ok(Self {
      flow,
      target,
      enumerator,
      notification_client: None,
      bindings_and_roles: Vec::new(),
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

    // Clear all bindings (will trigger Drop for each AudioBinding)
    self.bindings_and_roles.clear();

    self.enabled = false;
    Ok(())
  }

  fn flow_to_win_flow(flow: AudioFlow) -> EDataFlow {
    match flow {
      AudioFlow::Input => eCapture,
      AudioFlow::Output => eRender,
    }
  }

  fn target_to_scalar(target: &AtomicU32) -> f32 {
    (target.load(Ordering::SeqCst) as f32 / 100.0).clamp(0.0, 1.0)
  }

  pub fn bind_role(&mut self, role: ERole) -> Result<()> {
    // Remove existing binding if any (triggers Drop)
    if let Some(pos) = self
      .bindings_and_roles
      .iter()
      .position(|(r, _)| r.0 == role.0)
    {
      self.bindings_and_roles.remove(pos);
    }

    unsafe {
      let win_flow = Self::flow_to_win_flow(self.flow);
      let default_device =
        self.enumerator.GetDefaultAudioEndpoint(win_flow, role)?;

      // Let's activate using standard COM interface retrieval
      let endpoint_volume_obj: IAudioEndpointVolume =
        default_device.Activate(CLSCTX_INPROC_SERVER, None)?;

      let callback = VolumeNotificationCallback::new(
        endpoint_volume_obj.clone(),
        self.target.clone(),
        self.context_guid,
      );
      let callback_interface: IAudioEndpointVolumeCallback = callback.into();

      endpoint_volume_obj.RegisterControlChangeNotify(&callback_interface)?;

      self.bindings_and_roles.push((
        role,
        AudioBinding {
          endpoint: endpoint_volume_obj,
          callback: callback_interface,
        },
      ));
    }

    Ok(())
  }

  pub fn force_to_target(&self) {
    let val = Self::target_to_scalar(&self.target);
    for (_, binding) in &self.bindings_and_roles {
      unsafe {
        let _ = binding
          .endpoint
          .SetMasterVolumeLevelScalar(val, &self.context_guid);
      }
    }
  }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeNotificationCallback {
  endpoint: IAudioEndpointVolume,
  target: Arc<AtomicU32>,
  context_guid: GUID,
}

impl VolumeNotificationCallback {
  fn new(
    endpoint: IAudioEndpointVolume,
    target: Arc<AtomicU32>,
    context_guid: GUID,
  ) -> Self {
    Self {
      endpoint,
      target,
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

    let target_val = AudioEnforcer::target_to_scalar(&self.target);

    if (data.fMasterVolume - target_val).abs() > 0.005 {
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
      unsafe {
        let _ = PostThreadMessageW(
          self.main_thread_id,
          WM_WAKEUP,
          WPARAM(0),
          LPARAM(0),
        );
      }
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
