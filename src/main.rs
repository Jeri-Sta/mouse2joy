use evdev: :{
    uinput:: VirtualDevice, uinput::VirtualDeviceBuilder, AbsInfo, AbsoluteAxisType, Device,
    EventType, InputEvent, InputEventKind, Key, RelativeAxisType, UinputAbsSetup,
};
use std::fs;
use thiserror::Error;
use log::{info, warn, error, LevelFilter};
use env_logger::Builder;

mod configuration;
use configuration::Config;

const VJOYSTICK_NAME: &str = "mouse2joy";

// virtual steering wheel buttons (relevant to steering wheels)
static KEYS: [Key; 6] = [
    Key::BTN_SELECT,
    Key::BTN_START,
    Key::BTN_TL,
    Key::BTN_TR,
    Key::BTN_TL2,
    Key::BTN_TR2,
];

#[derive(Error, Debug)]
pub enum Mouse2JoyError {
    #[error("Failed to find a mouse device.  Make sure you are running the application with root priviledges.")]
    NoMouseError,

    #[error("Failed to read a mouse input")]
    FailedToReadInput,
}

fn main() -> Result<(), Mouse2JoyError> {

    // initialize logger
    Builder::new()
        .filter_level(LevelFilter:: Trace)
        .init();

    let conf = load_config();
    info!("sensitivity: {}", conf.sensitivity);
    
    // find all input devices that can be used as a mouse
    let mut mouse_devices:  Vec<Device> = fs::read_dir("/dev/input")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.path().into_os_string().to_str().map(String::from))
        .filter_map(|path| {
            Device::open(&path)
                .ok()
                .filter(|device| device.supported_events().contains(EventType::RELATIVE))
        })
        .collect();

    if mouse_devices.is_empty() {
        error!("{}", Mouse2JoyError::NoMouseError);
        return Err(Mouse2JoyError:: NoMouseError);
    }

    // ask user which mouse to use
    if !(mouse_devices.len() == 1) {
        println!("Several mouses detected, please select one:");
        for (i, mouse) in mouse_devices.iter().enumerate() {
            println!("{}: {}", i + 1, mouse.name().unwrap_or("Unknown Device"));
        }
    }

    let index = input_in_range(1, mouse_devices.len());
    let mut mouse = mouse_devices. remove(index - 1);
    info!("Using \"{}\" as input device", mouse.name().unwrap_or("Unknown Device"));

    // ungrab unwanted mouse devices
    for mut device in mouse_devices {
        device
            .ungrab()
            .unwrap_or_else(|e| warn!("Failed to ungrab device:  {}", e));
    }

    // set up virtual steering wheel with 900 degree rotation
    // Range: -4500 to 4500 (representing -900 to +900 degrees)
    // fuzz=0 and flat=0 for smooth input without deadzone
    let axis_info = AbsInfo::new(
        0,              // value (center)
        -4500,          // range_min (left extreme)
        4500,           // range_max (right extreme)
        0,              // fuzz:  0 for no deadzone
        0,              // flat: 0 for no deadzone
        0               // resolution: 0 for raw values
    );
    let mut steering_wheel = create_steering_wheel(axis_info, VJOYSTICK_NAME).unwrap();
    info!("Virtual steering wheel created (900 degree rotation - smooth, no deadzone)");

    // fetch events and send them through to virtual steering wheel
    let min:  i32 = -4500;
    let max: i32 = 4500;
    let mut steering_position: i32 = 0;
    
    loop {
        match mouse.fetch_events() {
            Ok(events) => {
                for ev in events {
                    if ev.kind() == InputEventKind::RelAxis(RelativeAxisType::REL_X) {
                        // Apply sensitivity multiplier from config
                        let delta = ev.value() * (conf.sensitivity as i32);
                        steering_position += delta;
                        
                        // Clamp to steering wheel range
                        if steering_position < min {
                            steering_position = min
                        } else if steering_position > max {
                            steering_position = max
                        }
                        
                        let ev = InputEvent::new(
                            EventType::ABSOLUTE,
                            AbsoluteAxisType::ABS_X.0,
                            steering_position,
                        );
                        
                        match steering_wheel.emit(&[ev]) {
                          Ok(_) => {
                            info! ("Steering:  {}", steering_position);
                          },
                          Err(e) => {
                            warn!("Failed to emit steering wheel event: {}", e);
                            continue;
                          }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch mouse events:  {}", e);
                continue;
            }
        }
    }
}

fn create_steering_wheel(abs_info: AbsInfo, name: &str) -> std::io::Result<VirtualDevice> {
    // Only use ABS_X for steering wheel rotation
    let abs_x = UinputAbsSetup:: new(AbsoluteAxisType:: ABS_X, abs_info);

    let mut keys = evdev:: AttributeSet::new();
    for button in KEYS {
        keys.insert(button)
    }

    let steering_wheel = VirtualDeviceBuilder::new()?
        .name(name)
        .with_absolute_axis(&abs_x)?
        .with_keys(&keys)?
        .build()?;

    Ok(steering_wheel)
}

// ask user for a usize input within a given range
fn input_in_range(min: usize, max: usize) -> usize {
    let mut input = String::new();

    loop {
        input. clear();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        match input.trim().parse::<usize>() {
            Ok(index) if index >= min && index <= max => {
                return index;
            }
            _ => {
                println!(
                    "Invalid selection. Please enter a number between {} and {}",
                    min, max
                );
                continue;
            }
        }
    }
}

fn load_config() -> Config {
    if Config:: exists() {
      match Config::load() {
        Ok(conf) => {
          info!("Using configuration file {}", Config::path());
          conf
        }
        Err(_) => {
          warn!("Problem laoding the configuration, using default");
          Config::default()
        }
      }
    } else {
      info! ("No configuration found, using default");
      Config::default()
    }
}
