extern crate dbus_bytestream;
use self::dbus_bytestream::message;

extern crate dbus_serialize;
use self::dbus_serialize::types::Variant;

use super::error::Error;
use super::value::{BasicValue, Marshal, Value};

#[derive(Debug)]
pub struct Message {
    pub message: message::Message,
}

pub enum MessageType {
    Error,
    Invalid,
    MethodCall,
    MethodReturn,
    Signal,
}

impl Message {
    pub fn new(message: message::Message) -> Message {
        Message {
            message: message,
        }
    }

    pub fn new_method_call(dest: &str, path: &str, iface: &str, method: &str) -> Message {
        Message {
            message: message::create_method_call(dest, path, iface, method),
        }
    }

    pub fn new_signal(path: &str, iface: &str, method: &str) -> Message {
        Message {
            message: message::create_signal(path, iface, method),
        }
    }

    pub fn error_message(&self, name: &str) -> Message {
        Message {
            message: message::create_error(name, self.message.serial),
        }
    }

    pub fn return_message(&self) -> Message {
        Message {
            message: message::create_method_return(self.message.serial),
        }
    }

    pub fn add_argument(self, arg: &Marshal) -> Message {
        Message {
            message: self.message.add_arg(arg),
        }
    }

    pub fn message_type(&self) -> MessageType {
        match self.message.message_type {
            message::MESSAGE_TYPE_ERROR         => MessageType::Error,
            message::MESSAGE_TYPE_INVALID       => MessageType::Invalid,
            message::MESSAGE_TYPE_METHOD_CALL   => MessageType::MethodCall,
            message::MESSAGE_TYPE_METHOD_RETURN => MessageType::MethodReturn,
            message::MESSAGE_TYPE_SIGNAL        => MessageType::Signal,
            _                                   => MessageType::Invalid,
        }
    }

    fn _extract_string(v: &Variant) -> Option<String> {
        if let Value::BasicValue(BasicValue::String(ref s)) = *v.object {
            Some(s.clone())
        } else {
            None
        }
    }

    fn _get_header_string(message: &message::Message, header: u8) -> Option<String> {
        message.get_header(header)
            .and_then(Self::_extract_string)
    }

    pub fn interface(&self) -> Option<String> {
        Self::_get_header_string(&self.message, message::HEADER_FIELD_INTERFACE)
    }

    pub fn path(&self) -> Option<String> {
        Self::_get_header_string(&self.message, message::HEADER_FIELD_PATH)
    }

    pub fn member(&self) -> Option<String> {
        Self::_get_header_string(&self.message, message::HEADER_FIELD_MEMBER)
    }

    pub fn values(&self) -> Result<Option<Vec<Value>>, Error> {
        Ok(try!(self.message.get_body()))
    }
}
