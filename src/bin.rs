use std::thread;
use std::fs::File;
use std::io::Write;
use std::num::ParseIntError;
use quicli::prelude::*;
use structopt::StructOpt;
use signal_hook::{iterator::Signals, SIGINT};

use rtlsdr::{Device, get_devices};

const INFINITE_SAMPLES : usize = 0;
const AUTO_GAIN : i32 = -100;

fn parse_auto_int(src: &str) -> Result<i32, ParseIntError> {
    if src == "auto" {
        Ok(AUTO_GAIN)
    } else {
        i32::from_str_radix(src, 10)
    }
}

fn parse_infinite_int(src: &str) -> Result<usize, ParseIntError> {
    if src == "infinite" {
        Ok(INFINITE_SAMPLES)
    } else {
        usize::from_str_radix(src, 10)
    }
}

#[derive(Debug, StructOpt)]
struct Cli {
    /// Frequency to tune to (Hz)
    #[structopt(long = "frequency", short = "f", default_value = "100000000")]
    frequency: u32,

    /// Sample rate (Hz)
    #[structopt(long = "sample-rate", short = "s", default_value = "2048000")]
    sample_rate: u32,

    /// Device index
    #[structopt(long = "device-index", short = "d", default_value = "0")]
    device_index: u32,

    /// Gain (0 for auto)
    #[structopt(long = "gain", short = "g", default_value = "auto", parse(try_from_str = "parse_auto_int"))]
    gain: i32,

    /// PPM Error
    #[structopt(long = "ppm-error", short = "p", default_value = "0")]
    ppm_error: i32,

    /// Output block size
    #[structopt(long = "output-block-size", short = "b", default_value = "262144")]
    output_block_size: u32,
    
    /// Number of samples to read
    #[structopt(long = "samples", short = "n", default_value = "infinite", parse(try_from_str = "parse_infinite_int"))]
    samples: usize,


    /// filename for output
    file: String,

    #[structopt(flatten)]
    verbosity: Verbosity,
}

#[derive(Debug)]
struct TextError {
    text: String,
}

impl TextError {
    pub fn new(text: String) -> Self {
        TextError { text }
    }
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl std::error::Error for TextError {
    fn description(&self) -> &str {
        &self.text
    }

    fn cause(&self) -> Option<&std::error::Error> {
        None
    }
}

fn main() -> CliResult {
    let args = Cli::from_args();
    args.verbosity.setup_env_logger("rtlsdr")?;

    let devices = get_devices();
    if devices.len() == 0 {
        Err(TextError::new("No supported RTLSDR devices found.".to_string()))?;
    }
    info!("Found {} device(s):", devices.len());
    for d in devices {
        info!("Vendor: {}, Device: {}, Serial: {}", d.vendor, d.product, d.serial);
    }

    let mut device = Device::open(args.device_index).map_err( |e| TextError::new(format!("Could not open device: {}", e.to_string()).to_string()) )?;
    
    /* Set gain, frequency, sample rate, and reset the device. */
    if args.gain == AUTO_GAIN {
        device.set_tuner_gain_mode(0)?;
    } else {
        device.set_tuner_gain_mode(1)?;
        device.set_tuner_gain(args.gain)?;
    }
    if args.ppm_error != 0 {
        device.set_freq_correction(args.ppm_error)?;
    }
    device.set_center_freq(args.frequency)?;
    device.set_sample_rate(args.sample_rate)?;
    device.reset_buffer()?;

    info!("Gain reported by device: {}", f64::from(device.get_tuner_gain())/10.0);

    let mut file = File::create(args.file.clone())?;
    info!("Reading data");
    setup_signal_handler(device.clone())?;

    let mut closure_device = device.clone();
    let mut total_bytes_written = 0;
    device.read_async(args.output_block_size, move |data| {
        let bytes_written = file.write(data).unwrap();
        total_bytes_written += bytes_written;
        trace!("Wrote {} bytes of data ({} total)", bytes_written, total_bytes_written);
        if bytes_written != data.len() {
            trace!("Wrote fewer bytes than was available. Cancelling.");
            closure_device.cancel_async().unwrap();
        }

        if args.samples > 0 && total_bytes_written > args.samples {
            debug!("Read required samples. Exiting.");
            closure_device.cancel_async().unwrap();
        }
    })?;

    Ok(())
}

fn setup_signal_handler(device: Device) -> Result<(), Error> {
    let signals = Signals::new(&[SIGINT])?;

    thread::spawn(move || {
        let mut d = device;
        for sig in signals.forever() {
            println!("Received signal {:?}", sig);
            d.cancel_async().unwrap();
        }
    });

    Ok(())
}