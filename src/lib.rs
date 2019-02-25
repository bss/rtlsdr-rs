use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use log::{info, debug, trace};
use librtlsdr_sys::*;
use std::ffi::CStr;
use std::io::Error;

#[derive(Debug, Clone)]
pub struct Device {
    device: *mut rtlsdr_dev_t,
    has_closed_device: Arc<AtomicBool>,
}
unsafe impl Send for Device {}

impl Device {
    pub fn open(device_index: u32) -> Result<Self, Error> {
        let mut device_ptr = 0 as *mut rtlsdr_dev_t;
        info!("Opening device: {}", device_index);
        let err_val = unsafe { rtlsdr_open((&mut device_ptr) as *mut _ as *mut *mut rtlsdr_dev_t, device_index) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(Device { device: device_ptr, has_closed_device: Arc::new(AtomicBool::new(false)) })
    }

    pub fn set_tuner_gain_mode(&mut self, tuner_gain_mode: i32) -> Result<(), Error> {
        info!("Setting tuner gain mode to {}", tuner_gain_mode);
        let err_val = unsafe { rtlsdr_set_tuner_gain_mode(self.device, tuner_gain_mode) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn get_tuner_gain(&mut self) -> i32 {
        unsafe { rtlsdr_get_tuner_gain(self.device) }
    }

    pub fn set_tuner_gain(&mut self, tuner_gain: i32) -> Result<(), Error> {
        info!("Setting tuner gain to {}", tuner_gain);
        let err_val = unsafe { rtlsdr_set_tuner_gain(self.device, tuner_gain) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn set_freq_correction(&mut self, ppm_error: i32) -> Result<(), Error> {
        info!("Setting frequency correction to {}", ppm_error);
        let err_val = unsafe { rtlsdr_set_freq_correction(self.device, ppm_error) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn set_center_freq(&mut self, frequency: u32) -> Result<(), Error> {
        info!("Setting frequency to {}", frequency);
        let err_val = unsafe { rtlsdr_set_center_freq(self.device, frequency) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), Error> {
        info!("Setting sample rate to {}", sample_rate);
        let err_val = unsafe { rtlsdr_set_sample_rate(self.device, sample_rate) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn reset_buffer(&mut self) -> Result<(), Error> {
        debug!("Resetting buffer");
        let err_val = unsafe { rtlsdr_reset_buffer(self.device) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn read_async<CB>(&mut self, output_block_size: u32, callback: CB) -> Result<(), Error>
    where CB: FnMut(&[u8]) + 'static {
        let boxed_callback = Box::new(callback);
        let tmp: Box<Box<AsyncClosureReader>> = Box::new(Box::new(AsyncClosureReader::new(boxed_callback)));
        let err_val = unsafe { rtlsdr_read_async(self.device, Some(async_callback), Box::into_raw(tmp) as *mut Box<AsyncClosureReader> as *mut std::ffi::c_void, 0, output_block_size) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn cancel_async(&mut self) -> Result<(), Error> {
        debug!("Cancelling async read");
        let err_val = unsafe { rtlsdr_cancel_async(self.device) };
        if err_val != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        let has_closed_device_clone = self.has_closed_device.clone();
        if has_closed_device_clone.compare_and_swap(false, true, Ordering::Relaxed) == false {
            debug!("Closing device");
            let err_val = unsafe { rtlsdr_close(self.device) };
            if err_val != 0 {
                return Err(Error::last_os_error());
            }
        } else {
            trace!("Device already closed.")
        }
        Ok(())
    }
}

pub struct AsyncClosureReader {
    callback: Box<FnMut(&[u8])>,
}

impl AsyncClosureReader {
    pub fn new(callback: Box<FnMut(&[u8])>) -> Self {
        AsyncClosureReader { callback }
    }
}

extern fn async_callback(buf: *mut ::std::os::raw::c_uchar, len: u32, boxed_ctx: *mut ::std::os::raw::c_void) {
    let buffer;
    let mut closure_reader: Box<Box<AsyncClosureReader>>;
    unsafe {
        closure_reader = Box::from_raw(boxed_ctx as *mut Box<AsyncClosureReader>);
        buffer = std::slice::from_raw_parts(buf, len as usize);
    }
    (closure_reader.callback)(buffer);

    // Make sure C code owns our pointer again so we don't segfault
    Box::into_raw(closure_reader);
}

impl Drop for Device {
    fn drop(&mut self) {
        trace!("Device dropped, closing if no already closed.");
        self.close().unwrap()
    }
}

pub struct DeviceInfo {
    pub vendor: String,
    pub product: String,
    pub serial: String,
}

pub fn get_device_count() -> u32 {
    unsafe { rtlsdr_get_device_count() }
}

pub fn get_devices() -> Vec<DeviceInfo> {
    let mut devices = vec!();
    let device_count = get_device_count();
    for j in 0..device_count {
        match get_device(j) {
            Ok(d) => devices.push(d),
            Err(_) => {}
        }
    }
    devices
}

fn get_device(index: u32) -> Result<DeviceInfo, Error> {
    let vendor;
    let mut vendor_c : [std::os::raw::c_char; 256] = [0; 256];
    let vendor_c_ptr = vendor_c.as_mut_ptr();

    let product;
    let mut product_c : [std::os::raw::c_char; 256] = [0; 256];
    let product_c_ptr = product_c.as_mut_ptr();

    let serial;
    let mut serial_c : [std::os::raw::c_char; 256] = [0; 256];
    let serial_c_ptr = serial_c.as_mut_ptr();
    
    let err_num;

    unsafe {
        err_num = rtlsdr_get_device_usb_strings(index, vendor_c_ptr, product_c_ptr, serial_c_ptr);
        vendor = CStr::from_ptr(vendor_c_ptr).to_string_lossy().into_owned();
        product = CStr::from_ptr(product_c_ptr).to_string_lossy().into_owned();
        serial = CStr::from_ptr(serial_c_ptr).to_string_lossy().into_owned();
    };
    
    if err_num == 0 {
        Ok(DeviceInfo { vendor, product, serial })
    } else {
        Err(Error::last_os_error())
    }
}

