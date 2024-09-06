use thiserror::Error;

use crate::Message;

#[derive(Error, Debug)]
pub enum Error {
    #[error("serial port error")]
    Disconnect(#[from] serialport::Error),
    #[error("parse error")]
    ParseError(#[from] serde_json::Error),
    #[error("no devices found")]
    NoDevicesFound,

    //serial port stuff
    #[error("serial port error")]
    SerialError(#[from] SerialError),
    #[error("could not send command")]
    CommandSendError,
    #[error("unexpected response `{0:?}`")]
    UnexpectedResponse(Message),
    #[error("conversion error: {0}")]
    ConversionError(String),
    // returned from device
    #[error("command error: {0} {1:?}")]
    CommandError(String, Option<String>),
    #[error("device error: {0} {1:?}")]
    DeviceError(String, Option<String>),
}

#[derive(Error, Debug, Clone)]
pub enum SerialError {
    #[error("serial port closed")]
    SerialPortClosed,
    #[error("unhandled message: `{0}`")]
    UnhandledMessage(String),
    #[error("error reading: `{0}`")]
    ErrorReading(String),
}
