use cpal::traits::{DeviceTrait, HostTrait};

pub struct CpalDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub device: cpal::Device,
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown".into())
}

pub fn list_input_devices() -> Result<Vec<CpalDeviceInfo>, cpal::Error> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_input_device().map(|d| device_name(&d));

    Ok(host
        .input_devices()?
        .map(|device| {
            let name = device_name(&device);
            CpalDeviceInfo {
                is_default: Some(&name) == default_name.as_ref(),
                name,
                device,
            }
        })
        .collect())
}

pub fn list_output_devices() -> Result<Vec<CpalDeviceInfo>, cpal::Error> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_output_device().map(|d| device_name(&d));

    Ok(host
        .output_devices()?
        .map(|device| {
            let name = device_name(&device);
            CpalDeviceInfo {
                is_default: Some(&name) == default_name.as_ref(),
                name,
                device,
            }
        })
        .collect())
}

pub fn find_input_device(name: &str) -> Option<cpal::Device> {
    list_input_devices()
        .ok()?
        .into_iter()
        .find(|d| d.name == name)
        .map(|d| d.device)
}

pub(crate) fn describe(device: &cpal::Device) -> String {
    device_name(device)
}
