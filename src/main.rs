// MVP Rust CLI for reading temperature from NI USB-TC01 using NI-DAQmx C library
// (via the ni-daqmx-sys crate for safe FFI bindings)
// 
// Prerequisites:
// 1. NI-DAQmx driver installed (Windows recommended; Linux support is limited)
// 2. NI USB-TC01 connected and visible in NI Measurement & Automation Explorer (MAX)
//    → Device name is usually "Dev1" (or "Dev2", etc.). Physical channel = "DevX/ai0"
// 3. Thermocouple connected to TC+ / TC- terminals (any supported type: J, K, T, etc.)
//
// How to use:
//   cargo run -- --device Dev1 --type J --continuous
//   (or just `cargo run` for defaults: single read from Dev1, J-type, °C)

use clap::Parser;
use ni_daqmx_sys as daq; // direct C API bindings
use std::ffi::CString;
use std::ptr;

#[derive(Parser, Debug)]
#[command(author, version, about = "Read temperature from NI USB-TC01 via DAQmx")]
struct Args {
    /// Device name as shown in NI MAX (e.g. Dev1)
    #[arg(short, long, default_value = "Dev1")]
    device: String,

    /// Thermocouple type (J, K, T, E, N, R, S, B)
    #[arg(short, long, default_value = "J")]
    r#type: String,

    /// Temperature units (C, F, K)
    #[arg(short, long, default_value = "C")]
    units: String,

    /// Run in continuous mode (print every \~1s until Ctrl+C)
    #[arg(short, long)]
    continuous: bool,
}

fn main() {
    let args = Args::parse();

    // Build physical channel name (USB-TC01 always uses ai0 for the thermocouple)
    let physical_channel = format!("{}/ai0", args.device);
    let physical_channel_c = CString::new(physical_channel).expect("Invalid physical channel name");

    // Map CLI args to DAQmx constants
    let tc_type = match args.r#type.to_uppercase().as_str() {
        "J" => daq::DAQmx_Val_J_Type_TC,
        "K" => daq::DAQmx_Val_K_Type_TC,
        "T" => daq::DAQmx_Val_T_Type_TC,
        "E" => daq::DAQmx_Val_E_Type_TC,
        "N" => daq::DAQmx_Val_N_Type_TC,
        "R" => daq::DAQmx_Val_R_Type_TC,
        "S" => daq::DAQmx_Val_S_Type_TC,
        "B" => daq::DAQmx_Val_B_Type_TC,
        _ => {
            eprintln!("Warning: Unknown TC type '{}', defaulting to J", args.r#type);
            daq::DAQmx_Val_J_Type_TC
        }
    };

    let units = match args.units.to_uppercase().as_str() {
        "C" => daq::DAQmx_Val_DegC,
        "F" => daq::DAQmx_Val_DegF,
        "K" => daq::DAQmx_Val_Kelvins,
        _ => {
            eprintln!("Warning: Unknown units '{}', defaulting to °C", args.units);
            daq::DAQmx_Val_DegC
        }
    };

    // Task setup (unsafe because we're calling the raw C API)
    unsafe {
        let mut task_handle: daq::TaskHandle = ptr::null_mut();
        let task_name = CString::new("TC01_Temp_Task").unwrap();

        // 1. Create task
        let mut err = daq::DAQmxCreateTask(task_name.as_ptr(), &mut task_handle);
        if err != 0 {
            handle_error(task_handle, err);
            return;
        }

        // 2. Create thermocouple channel (USB-TC01 uses built-in CJC)
        let chan_name = CString::new("").unwrap(); // empty = auto-assign
        let custom_scale = CString::new("").unwrap();

        err = daq::DAQmxCreateAIThrmcplChan(
            task_handle,
            physical_channel_c.as_ptr(),
            chan_name.as_ptr(),
            units,
            tc_type,
            daq::DAQmx_Val_BuiltIn, // built-in cold-junction compensation (recommended for TC01)
            0.0,                     // CJC value is ignored when using BuiltIn
            custom_scale.as_ptr(),
        );
        if err != 0 {
            handle_error(task_handle, err);
            return;
        }

        // 3. Start the task
        err = daq::DAQmxStartTask(task_handle);
        if err != 0 {
            handle_error(task_handle, err);
            return;
        }

        println!("✅ Connected to NI USB-TC01 ({})", physical_channel_c.to_str().unwrap());
        println!("   TC type: {}, Units: {}", args.r#type.to_uppercase(), args.units.to_uppercase());
        println!("   Press Ctrl+C to exit in continuous mode\n");

        // Read loop
        loop {
            let mut temperature: f64 = 0.0;
            err = daq::DAQmxReadAnalogScalarF64(
                task_handle,
                10.0,           // timeout in seconds
                &mut temperature,
                ptr::null_mut(), // reserved
            );

            if err != 0 {
                handle_error(task_handle, err);
                break;
            }

            println!("🌡️  Temperature: {:.2} °{}", temperature, args.units.to_uppercase());

            if !args.continuous {
                break;
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        // Cleanup
        let _ = daq::DAQmxStopTask(task_handle);
        let _ = daq::DAQmxClearTask(task_handle);
    }
}

unsafe fn handle_error(task_handle: daq::TaskHandle, err: i32) {
    let mut err_buf = [0u8; 2048];
    daq::DAQmxGetExtendedErrorInfo(err_buf.as_mut_ptr() as *mut i8, err_buf.len() as u32);
    let msg = std::ffi::CStr::from_ptr(err_buf.as_ptr() as *const i8)
        .to_string_lossy()
        .into_owned();

    eprintln!("❌ DAQmx Error {}: {}", err, msg);

    if !task_handle.is_null() {
        let _ = daq::DAQmxClearTask(task_handle);
    }
}
