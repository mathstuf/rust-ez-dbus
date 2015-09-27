extern crate machine_id;
use self::machine_id::MachineId;

use super::arguments::DBusArguments;
use super::connection::DBusConnection;
use super::error::DBusError;
use super::message::DBusMessage;
use super::value::{DBusBasicValue, DBusDictionary, DBusSignature, DBusValue};

use std::cell::{Ref, RefCell};
use std::collections::btree_map::{BTreeMap, Entry};
use std::collections::HashMap;
use std::rc::Rc;

type DBusMap<T> = BTreeMap<String, T>;

pub struct DBusArgument {
    name: String,
    signature: String,
}

impl DBusArgument {
    pub fn new(name: &str, sig: &str) -> DBusArgument {
        DBusArgument {
            name: name.to_owned(),
            signature: sig.to_owned(),
        }
    }
}

pub struct DBusAnnotation {
    name: String,
    value: String,
}
type DBusAnnotations = Vec<DBusAnnotation>;

impl DBusAnnotation {
    pub fn new(name: &str, value: &str) -> DBusAnnotation {
        DBusAnnotation {
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }
}

pub struct DBusErrorMessage {
    name: String,
    message: String,
}

impl DBusErrorMessage {
    pub fn new(name: &str, message: &str) -> DBusErrorMessage {
        DBusErrorMessage {
            name: name.to_owned(),
            message: message.to_owned(),
        }
    }
}

pub type DBusMethodResult = Result<Vec<DBusValue>, DBusErrorMessage>;
pub type DBusMethodHandler = Box<FnMut(&mut DBusMessage) -> DBusMethodResult>;

pub struct DBusMethod {
    in_args: Vec<DBusArgument>,
    out_args: Vec<DBusArgument>,
    cb: DBusMethodHandler,
    anns: DBusAnnotations,
}

impl DBusMethod {
    pub fn new<F>(cb: F) -> DBusMethod
        where F: FnMut(&mut DBusMessage) -> DBusMethodResult + 'static {
        DBusMethod {
            in_args: vec![],
            out_args: vec![],
            cb: Box::new(cb),
            anns: vec![],
        }
    }

    pub fn add_argument(mut self, arg: DBusArgument) -> DBusMethod {
        self.in_args.push(arg);

        self
    }

    pub fn add_result(mut self, arg: DBusArgument) -> DBusMethod {
        self.out_args.push(arg);

        self
    }

    pub fn annotate(mut self, ann: DBusAnnotation) -> DBusMethod {
        self.anns.push(ann);

        self
    }
}

pub type DBusPropertyGetResult = Result<DBusValue, DBusErrorMessage>;
pub type DBusPropertySetResult = Result<(), DBusErrorMessage>;

pub trait DBusPropertyReadHandler {
    fn get(&self) -> DBusPropertyGetResult;
}

pub trait DBusPropertyWriteHandler {
    fn set(&self, &DBusValue) -> DBusPropertySetResult;
}

pub trait DBusPropertyReadWriteHandler {
    fn get(&self) -> DBusPropertyGetResult;
    fn set(&self, &DBusValue) -> DBusPropertySetResult;
}

enum PropertyAccess {
    RO(Box<DBusPropertyReadHandler>),
    RW(Box<DBusPropertyReadWriteHandler>),
    WO(Box<DBusPropertyWriteHandler>),
}

pub struct DBusProperty {
    signature: DBusSignature,
    access: PropertyAccess,
    anns: DBusAnnotations,
}

impl DBusProperty {
    fn new(sig: DBusSignature, access: PropertyAccess) -> DBusProperty {
        DBusProperty {
            signature: sig,
            access: access,
            anns: vec![],
        }
    }

    pub fn new_ro(sig: DBusSignature, access: Box<DBusPropertyReadHandler>) -> DBusProperty {
        DBusProperty::new(sig, PropertyAccess::RO(access))
    }

    pub fn new_rw(sig: DBusSignature, access: Box<DBusPropertyReadWriteHandler>) -> DBusProperty {
        DBusProperty::new(sig, PropertyAccess::RW(access))
    }

    pub fn new_wo(sig: DBusSignature, access: Box<DBusPropertyWriteHandler>) -> DBusProperty {
        DBusProperty::new(sig, PropertyAccess::WO(access))
    }

    pub fn annotate(mut self, ann: DBusAnnotation) -> DBusProperty {
        self.anns.push(ann);

        self
    }
}

pub struct DBusSignal {
    args: Vec<DBusArgument>,
    anns: DBusAnnotations,
}

impl DBusSignal {
    pub fn new() -> DBusSignal {
        DBusSignal {
            args: vec![],
            anns: vec![],
        }
    }

    pub fn add_argument(mut self, arg: DBusArgument) -> DBusSignal {
        self.args.push(arg);

        self
    }

    pub fn annotate(mut self, ann: DBusAnnotation) -> DBusSignal {
        self.anns.push(ann);

        self
    }
}

pub struct DBusInterface {
    methods: DBusMap<DBusMethod>,
    properties: DBusMap<DBusProperty>,
    signals: DBusMap<DBusSignal>,
}

impl DBusInterface {
    pub fn new() -> DBusInterface {
        DBusInterface {
            methods: DBusMap::new(),
            properties: DBusMap::new(),
            signals: DBusMap::new(),
        }
    }

    pub fn add_method(mut self, name: &str, method: DBusMethod) -> DBusInterface {
        self.methods.insert(name.to_owned(), method);

        self
    }

    pub fn add_property(mut self, name: &str, property: DBusProperty) -> DBusInterface {
        self.properties.insert(name.to_owned(), property);

        self
    }

    pub fn get_property(&self, name: &str) -> Option<&DBusProperty> {
        self.properties.get(name)
    }

    pub fn add_signal(mut self, name: &str, signal: DBusSignal) -> DBusInterface {
        self.signals.insert(name.to_owned(), signal);

        self
    }

    fn _require_property(&self, name: &str) -> Result<&DBusProperty, DBusErrorMessage> {
        self.properties.get(name).ok_or(
            DBusErrorMessage::new("org.freedesktop.DBus.Error.UnknownProperty",
                                  &format!("unknown property: {}", name)))
    }

    pub fn get_property_value(&self, name: &str) -> DBusMethodResult {
        self._require_property(name).and_then(|prop| {
            match prop.access {
                // TODO: Verify that the signature matches the return.
                PropertyAccess::RO(ref ro) => ro.get().map(|v| vec![v]),
                PropertyAccess::RW(ref rw) => rw.get().map(|v| vec![v]),
                PropertyAccess::WO(_) =>
                    Err(DBusErrorMessage {
                        name: "org.freedesktop.DBus.Error.Failed".to_owned(),
                        message: format!("property is write-only: {}", name),
                    }),
            }
        })
    }

    pub fn set_property_value(&self, name: &str, value: &DBusValue) -> DBusMethodResult {
        self._require_property(name).and_then(|prop| {
            match prop.access {
                PropertyAccess::WO(ref wo) => wo.set(value).map(|_| vec![]),
                PropertyAccess::RW(ref rw) => rw.set(value).map(|_| vec![]),
                PropertyAccess::RO(_) =>
                    Err(DBusErrorMessage::new("org.freedesktop.DBus.Error.Failed",
                                              &format!("property is read-only: {}", name))),
            }
        })
    }

    pub fn get_property_map(&self) -> DBusDictionary {
        DBusDictionary::new(self.properties.iter().map(|(k, v)| {
            match v.access {
                // TODO: Message that failures occurred?
                // TODO: Verify that the signature matches the return.
                PropertyAccess::RO(ref ro) => ro.get().ok(),
                PropertyAccess::RW(ref rw) => rw.get().ok(),
                PropertyAccess::WO(_)      => None,
            }.map(|v| {
                (DBusBasicValue::String(k.clone()), v)
            })
        }).filter_map(|a| a).collect::<HashMap<DBusBasicValue, DBusValue>>())
    }
}

type InterfaceMap = Rc<RefCell<DBusMap<DBusInterface>>>;

pub struct DBusInterfaceMap {
    map: InterfaceMap,
    finalized: bool,
}

impl DBusInterfaceMap {
    pub fn new() -> DBusInterfaceMap {
        DBusInterfaceMap {
            map: Rc::new(RefCell::new(DBusMap::new())),
            finalized: false,
        }
    }

    // Marked as mut for intent; Rc<> doesn't require it though.
    #[allow(unused_mut)]
    pub fn add_interface(mut self, name: &str, iface: DBusInterface) -> Result<DBusInterfaceMap, DBusError> {
        if self.finalized {
            return Err(DBusError::InterfaceMapFinalized(name.to_owned()));
        }

        {
            let mut map = self.map.borrow_mut();

            match map.entry(name.to_owned()) {
                Entry::Vacant(v)    => {
                    v.insert(iface);

                    Ok(())
                },
                Entry::Occupied(_)  => Err(DBusError::InterfaceAlreadyRegistered(name.to_owned())),
            }
        }.map(|_| self)
    }

    fn ping() -> DBusMethodResult {
        Ok(vec![])
    }

    fn get_machine_id() -> DBusMethodResult {
        let mid = format!("{}", MachineId::get());
        Ok(vec![DBusValue::BasicValue(DBusBasicValue::String(mid))])
    }

    fn _require_interface<'a>(map: &'a Ref<'a, DBusMap<DBusInterface>>, name: &str) -> Result<&'a DBusInterface, DBusErrorMessage> {
        map.get(name).ok_or(
            DBusErrorMessage {
                name: "org.freedesktop.DBus.Error.UnknownInterface".to_owned(),
                message: format!("unknown interface: {}", name),
            })
    }

    fn get_property(map: &InterfaceMap, m: &mut DBusMessage) -> DBusMethodResult {
        let values = try!(DBusArguments::new(m));
        let iface = try!(values.extract_string(0));
        let property = try!(values.extract_string(1));

        Self::_require_interface(&map.borrow(), iface).and_then(|iface| {
            iface.get_property_value(property)
        })
    }

    fn set_property(map: &mut InterfaceMap, m: &mut DBusMessage) -> DBusMethodResult {
        let values = try!(DBusArguments::new(m));
        let iface = try!(values.extract_string(0));
        let property = try!(values.extract_string(1));
        let value = try!(values.extract(2));

        Self::_require_interface(&map.borrow(), iface).and_then(|iface| {
            iface.set_property_value(property, value)
        })
    }

    fn get_all_properties(map: &InterfaceMap, m: &mut DBusMessage) -> DBusMethodResult {
        let values = try!(DBusArguments::new(m));
        let iface = try!(values.extract_string(0));

        Self::_require_interface(&map.borrow(), iface).map(|iface| {
            vec![DBusValue::Dictionary(iface.get_property_map())]
        })
    }

    pub fn finalize(mut self) -> Result<DBusInterfaceMap, DBusError> {
        self = try!(self.add_interface("org.freedesktop.DBus.Peer", DBusInterface::new()
            .add_method("Ping", DBusMethod::new(|_| Self::ping()))
            .add_method("GetMachineId", DBusMethod::new(|_| Self::get_machine_id())
                .add_result(DBusArgument::new("machine_uuid", "s")))
        ));

        let get_map = self.map.clone();
        let mut set_map = self.map.clone();
        let get_all_map = self.map.clone();

        self = try!(self.add_interface("org.freedesktop.DBus.Properties", DBusInterface::new()
            .add_method("Get", DBusMethod::new(move |m| Self::get_property(&get_map, m))
                .add_argument(DBusArgument::new("interface_name", "s"))
                .add_argument(DBusArgument::new("property_name", "s"))
                .add_result(DBusArgument::new("value", "v")))
            .add_method("Set", DBusMethod::new(move |m| Self::set_property(&mut set_map, m))
                .add_argument(DBusArgument::new("interface_name", "s"))
                .add_argument(DBusArgument::new("property_name", "s"))
                .add_result(DBusArgument::new("value", "v")))
            .add_method("GetAll", DBusMethod::new(move |m| Self::get_all_properties(&get_all_map, m))
                .add_argument(DBusArgument::new("interface_name", "s"))
                .add_result(DBusArgument::new("props", "{sv}")))
        ));

        // TODO: Add core interfaces.

        self.finalized = true;
        Ok(self)
    }

    pub fn handle(&self, conn: &DBusConnection, msg: &mut DBusMessage) -> Option<Result<(), ()>> {
        msg.call_headers().and_then(|hdrs| {
            let iface_name = hdrs.interface;
            let method_name = hdrs.method;
            self.map.borrow_mut().get_mut(&iface_name).and_then(|iface| iface.methods.get_mut(&method_name)).map(|method| {
                // TODO: Verify input argument signature.

                let msg = match (method.cb)(msg) {
                    Ok(vals) => {
                        vals.iter().fold(msg.return_message(), |msg, val| {
                            msg.add_argument(val)
                        })
                    },
                    Err(err) => msg.error_message(&err.name)
                                   .add_argument(&err.message),
                };

                // TODO: Verify that the signature matches the return.

                conn.send(msg)
                    .map(|_| ())
                    .map_err(|_| ())
            })
        })
    }
}
