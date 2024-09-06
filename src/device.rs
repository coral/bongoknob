use crate::error::Error;
use crate::{protocol, Command, Message};
use crossbeam::channel::{bounded, unbounded, Receiver, Sender};
use log::{error, info};
use serialport::{DataBits, FlowControl, Parity, SerialPort, SerialPortInfo, StopBits, TTYPort};
use std::fmt;
use std::io::Read;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AvailableDevice {
    port_info: serialport::SerialPortInfo,
    timeout: Duration,
}

impl fmt::Display for AvailableDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let serialport::SerialPortType::UsbPort(usb_info) = &self.port_info.port_type {
            write!(
                f,
                "Device: {} \nPort: {}",
                usb_info.product.as_deref().unwrap_or("Unknown"),
                self.port_info.port_name
            )
        } else {
            write!(f, "Device: Unknown \nPort: {}", self.port_info.port_name)
        }
    }
}

impl AvailableDevice {
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

fn enumerate() -> Result<Vec<SerialPortInfo>, Error> {
    let ports = serialport::available_ports()?;

    let res: Vec<SerialPortInfo> = ports
        .iter()
        .filter(|p| match &p.port_type {
            serialport::SerialPortType::UsbPort(port) => {
                (port.vid == 12346 && port.pid == 4097) || (port.vid == 9114 && port.pid == 32784)
            }
            _ => false,
        })
        .filter(|p| p.port_name.starts_with("/dev/tty"))
        .cloned()
        .collect();

    Ok(res)
}

pub fn discover() -> Result<Vec<AvailableDevice>, Error> {
    let ports = enumerate()?;

    if ports.is_empty() {
        return Err(Error::NoDevicesFound);
    }

    let devices: Vec<AvailableDevice> = ports
        .iter()
        .map(|p| AvailableDevice {
            port_info: p.clone(),
            timeout: Duration::from_millis(10),
        })
        .collect();

    Ok(devices)
}

pub fn connect(device: AvailableDevice) -> Result<Device, Error> {
    info!("Connecting to device: {:?}", device.port_info.port_name);
    let mut port = serialport::new(device.port_info.port_name, 115200)
        .data_bits(DataBits::Eight)
        .stop_bits(StopBits::One)
        .parity(Parity::None)
        .flow_control(FlowControl::None)
        .open_native()?;

    #[cfg(unix)]
    port.set_exclusive(false)
        .expect("Unable to set serial port exclusive to false");

    port.set_timeout(device.timeout)
        .expect("Failed to set port timeout");

    Ok(Device::create(port))
}

#[derive(Debug, Clone)]
pub struct Device {
    messages: Receiver<Message>,

    commands: Sender<(Command, Option<Sender<Result<Message, Error>>>)>,
}

impl Device {
    pub fn create(mut port: TTYPort) -> Device {
        let (msg_tx, msg_rx) = unbounded();
        let (cmd_tx, cmd_rx) = unbounded::<(Command, Option<Sender<Result<Message, Error>>>)>();

        thread::spawn(move || {
            let message_pipe = msg_tx;
            let mut buffer = Vec::new();
            let mut command_buffer = Vec::new();

            let mut line_buffer: Option<String> = None;

            loop {
                // process serial data from device
                let mut serial_buf = [0; 1000];
                match port.read(&mut serial_buf) {
                    Ok(t) => {
                        buffer.extend_from_slice(&serial_buf[..t]);
                        while let Some(pos) = buffer.iter().position(|&x| x == b'\n') {
                            let line: Vec<u8> = buffer.drain(..=pos).collect::<Vec<_>>();
                            let line = String::from_utf8(line).unwrap();

                            line_buffer = Some(line);
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(e) => error!("could not read from serial port: {:?}", e),
                }

                // check if there's any commands to process
                match cmd_rx.try_recv() {
                    Ok((command, tx)) => {
                        let cmd = command.to_string();

                        // add command response pipe to stack
                        match tx {
                            Some(tx) => command_buffer.push(tx),
                            None => {}
                        };
                        // TODO fix unwraps here
                        port.write_all(cmd.as_bytes()).unwrap();
                        port.write_all(b"\n").unwrap();
                        port.flush().unwrap();
                    }
                    Err(e) => match e {
                        crossbeam::channel::TryRecvError::Empty => {}
                        // bail out of event loop if command pipe disconnected
                        // we can assume the Device was dropped
                        crossbeam::channel::TryRecvError::Disconnected => {}
                    },
                }

                // process buffered message
                match line_buffer {
                    Some(ref line) => {
                        let message = protocol::Message::try_from(line.as_str());
                        match message {
                            Ok(message) => match message {
                                Message::Heartbeat(_) | Message::Event(_) => {
                                    message_pipe.send(message).unwrap();
                                }
                                Message::Error(e) => {
                                    if command_buffer.len() > 0 {
                                        command_buffer
                                            .remove(0)
                                            .send(Err(Error::CommandError(e.error, e.msg)))
                                            .unwrap();
                                    } else {
                                        let err = Error::DeviceError(e.error, e.msg);
                                        error!("device error: {}", err);
                                    }
                                }
                                _ => {
                                    if command_buffer.len() > 0 {
                                        command_buffer.remove(0).send(Ok(message)).unwrap();
                                    }
                                }
                            },
                            Err(e) => {
                                error!("could not parse message: {}", e);
                            }
                        }

                        line_buffer = None;
                    }
                    None => {}
                }
            }
        });

        Device {
            commands: cmd_tx,
            messages: msg_rx,
        }
    }

    pub fn command_response(&self, command: Command) -> Result<Message, Error> {
        let (tx, rx) = bounded(1);
        match self.commands.send((command, Some(tx))) {
            Ok(_) => {}
            Err(_) => return Err(Error::CommandSendError),
        }
        match rx.recv() {
            Ok(msg) => msg,
            Err(_) => Err(Error::CommandSendError),
        }
    }

    pub fn command(&self, command: Command) -> Result<(), Error> {
        match self.commands.send((command, None)) {
            Ok(_) => Ok(()),
            Err(_) => return Err(Error::CommandSendError),
        }
    }

    pub fn subscribe(&self) -> Receiver<Message> {
        self.messages.clone()
    }

    // GET
    pub fn get_settings(&self) -> Result<protocol::Settings, Error> {
        let v = self.command_response(Command::GetSettings)?;
        match v {
            Message::Settings(settings_root) => Ok(settings_root.settings),
            Message::Error(e) => Err(Error::CommandError(e.error, e.msg)),
            _ => Err(Error::UnexpectedResponse(v)),
        }
    }

    pub fn get_profiles(&self) -> Result<Vec<String>, Error> {
        let v = self.command_response(Command::GetProfiles)?;
        match v {
            Message::Profiles(p) => match p.profiles {
                Some(v) => return Ok(v),
                None => return Ok(Vec::new()),
            },
            Message::Error(e) => Err(Error::CommandError(e.error, e.msg)),
            _ => Err(Error::UnexpectedResponse(v)),
        }
    }

    pub fn get_profile(&self, profile: String) -> Result<protocol::Profile, Error> {
        let v = self.command_response(Command::GetProfile(profile))?;
        match v {
            Message::Profile(profile_root) => Ok(profile_root.profile),
            Message::Error(e) => Err(Error::CommandError(e.error, e.msg)),
            _ => Err(Error::UnexpectedResponse(v)),
        }
    }

    // SET
    pub fn set_screen(&self, data: protocol::ScreenData) -> Result<(), Error> {
        self.command(Command::SetScreen(data)).unwrap();
        Ok(())
    }

    /// Show a message on the device screen
    ///
    /// # Arguments
    ///
    /// * `title` - big text
    /// * `text` -  small text (doesn't seem to do shit right now?)
    /// * `duration` - The duration of the message in seconds
    pub fn set_message(
        &self,
        title: Option<String>,
        text: Option<String>,
        duration: Option<f32>,
    ) -> Result<(), Error> {
        let msg = crate::protocol::MessageDetails {
            title,
            text,
            duration,
        };
        self.command(Command::ShowMessage(msg)).unwrap();
        Ok(())
    }

    // MISC

    /// Save the settings and profiles to SPIFFs
    pub fn save_settings(&self) -> Result<(), Error> {
        self.command(Command::Save).unwrap();
        Ok(())
    }

    /// Reload the settings and profiles from SPIFFs
    pub fn load_settings(&self) -> Result<(), Error> {
        self.command(Command::Load).unwrap();
        Ok(())
    }

    /// Reset motor calibration
    pub fn recalibrate(&self) -> Result<(), Error> {
        self.command(Command::Recalibrate).unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands() {
        let devices = discover().unwrap();
        let device = connect(devices[0].clone()).unwrap();

        // try to get settings
        device.get_settings().unwrap();

        // try to get a profile that doesn't exist
        assert!(matches!(
            device.get_profile("This should not exist!*!*!*!!*".to_string()),
            Err(Error::CommandError(_, _))
        ));
    }
}
