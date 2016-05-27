extern crate machine_id;
use self::machine_id::MachineId;

use super::arguments::Arguments;
use super::connection::Connection;
use super::error::Error;
use super::message::Message;
use super::value::{BasicValue, Dictionary, Signature, Value};

use std::cell::{Ref, RefCell};
use std::collections::btree_map::{BTreeMap, Entry};
use std::collections::HashMap;
use std::rc::Rc;

type Map<T> = BTreeMap<String, T>;

pub struct Argument {
    name: String,
    signature: String,
}

impl Argument {
    pub fn new(name: &str, sig: &str) -> Argument {
        Argument {
            name: name.to_owned(),
            signature: sig.to_owned(),
        }
    }
}

pub struct Annotation {
    name: String,
    value: String,
}
type Annotations = Vec<Annotation>;

impl Annotation {
    pub fn new(name: &str, value: &str) -> Annotation {
        Annotation {
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }
}

pub struct ErrorMessage {
    name: String,
    message: String,
}

impl ErrorMessage {
    pub fn new(name: &str, message: &str) -> ErrorMessage {
        ErrorMessage {
            name: name.to_owned(),
            message: message.to_owned(),
        }
    }
}

pub type MethodResult = Result<Vec<Value>, ErrorMessage>;
pub type MethodHandler = Box<FnMut(&mut Message) -> MethodResult>;

pub struct Method {
    in_args: Vec<Argument>,
    out_args: Vec<Argument>,
    cb: MethodHandler,
    anns: Annotations,
}

impl Method {
    pub fn new<F>(cb: F) -> Method
        where F: FnMut(&mut Message) -> MethodResult + 'static {
        Method {
            in_args: vec![],
            out_args: vec![],
            cb: Box::new(cb),
            anns: vec![],
        }
    }

    pub fn add_argument(mut self, arg: Argument) -> Method {
        self.in_args.push(arg);

        self
    }

    pub fn add_result(mut self, arg: Argument) -> Method {
        self.out_args.push(arg);

        self
    }

    pub fn annotate(mut self, ann: Annotation) -> Method {
        self.anns.push(ann);

        self
    }
}

pub type PropertyGetResult = Result<Value, ErrorMessage>;
pub type PropertySetResult = Result<(), ErrorMessage>;

pub trait PropertyReadHandler {
    fn get(&self) -> PropertyGetResult;
}

pub trait PropertyWriteHandler {
    fn set(&self, &Value) -> PropertySetResult;
}

pub trait PropertyReadWriteHandler {
    fn get(&self) -> PropertyGetResult;
    fn set(&self, &Value) -> PropertySetResult;
}

enum PropertyAccess {
    RO(Box<PropertyReadHandler>),
    RW(Box<PropertyReadWriteHandler>),
    WO(Box<PropertyWriteHandler>),
}

pub struct Property {
    signature: Signature,
    access: PropertyAccess,
    anns: Annotations,
}

impl Property {
    fn new(sig: Signature, access: PropertyAccess) -> Property {
        Property {
            signature: sig,
            access: access,
            anns: vec![],
        }
    }

    pub fn new_ro(sig: Signature, access: Box<PropertyReadHandler>) -> Property {
        Property::new(sig, PropertyAccess::RO(access))
    }

    pub fn new_rw(sig: Signature, access: Box<PropertyReadWriteHandler>) -> Property {
        Property::new(sig, PropertyAccess::RW(access))
    }

    pub fn new_wo(sig: Signature, access: Box<PropertyWriteHandler>) -> Property {
        Property::new(sig, PropertyAccess::WO(access))
    }

    pub fn annotate(mut self, ann: Annotation) -> Property {
        self.anns.push(ann);

        self
    }
}

pub struct Signal {
    args: Vec<Argument>,
    anns: Annotations,
}

impl Signal {
    pub fn new() -> Signal {
        Signal {
            args: vec![],
            anns: vec![],
        }
    }

    pub fn add_argument(mut self, arg: Argument) -> Signal {
        self.args.push(arg);

        self
    }

    pub fn annotate(mut self, ann: Annotation) -> Signal {
        self.anns.push(ann);

        self
    }
}

pub struct Interface {
    methods: Map<Method>,
    properties: Map<Property>,
    signals: Map<Signal>,
}

impl Interface {
    pub fn new() -> Interface {
        Interface {
            methods: Map::new(),
            properties: Map::new(),
            signals: Map::new(),
        }
    }

    pub fn add_method(mut self, name: &str, method: Method) -> Interface {
        self.methods.insert(name.to_owned(), method);

        self
    }

    pub fn add_property(mut self, name: &str, property: Property) -> Interface {
        self.properties.insert(name.to_owned(), property);

        self
    }

    pub fn get_property(&self, name: &str) -> Option<&Property> {
        self.properties.get(name)
    }

    pub fn add_signal(mut self, name: &str, signal: Signal) -> Interface {
        self.signals.insert(name.to_owned(), signal);

        self
    }

    fn _require_property(&self, name: &str) -> Result<&Property, ErrorMessage> {
        self.properties.get(name).ok_or(
            ErrorMessage::new("org.freedesktop.DBus.Error.UnknownProperty",
                              &format!("unknown property: {}", name)))
    }

    pub fn get_property_value(&self, name: &str) -> MethodResult {
        self._require_property(name).and_then(|prop| {
            match prop.access {
                // TODO: Verify that the signature matches the return.
                PropertyAccess::RO(ref ro) => ro.get().map(|v| vec![v]),
                PropertyAccess::RW(ref rw) => rw.get().map(|v| vec![v]),
                PropertyAccess::WO(_) =>
                    Err(ErrorMessage {
                        name: "org.freedesktop.DBus.Error.Failed".to_owned(),
                        message: format!("property is write-only: {}", name),
                    }),
            }
        })
    }

    pub fn set_property_value(&self, name: &str, value: &Value) -> MethodResult {
        self._require_property(name).and_then(|prop| {
            match prop.access {
                PropertyAccess::WO(ref wo) => wo.set(value).map(|_| vec![]),
                PropertyAccess::RW(ref rw) => rw.set(value).map(|_| vec![]),
                PropertyAccess::RO(_) =>
                    Err(ErrorMessage::new("org.freedesktop.DBus.Error.Failed",
                                          &format!("property is read-only: {}", name))),
            }
        })
    }

    pub fn get_property_map(&self) -> Dictionary {
        Dictionary::new(self.properties.iter().map(|(k, v)| {
            match v.access {
                // TODO: Message that failures occurred?
                // TODO: Verify that the signature matches the return.
                PropertyAccess::RO(ref ro) => ro.get().ok(),
                PropertyAccess::RW(ref rw) => rw.get().ok(),
                PropertyAccess::WO(_)      => None,
            }.map(|v| {
                (BasicValue::String(k.clone()), v)
            })
        }).filter_map(|a| a).collect::<HashMap<BasicValue, Value>>())
    }
}

type InterfaceMap = Rc<RefCell<Map<Interface>>>;
pub type ChildrenList = Rc<RefCell<Vec<String>>>;

fn require_interface<'a>(map: &'a Ref<'a, Map<Interface>>, name: &str) -> Result<&'a Interface, ErrorMessage> {
    map.get(name).ok_or(
        ErrorMessage {
            name: "org.freedesktop.DBus.Error.UnknownInterface".to_owned(),
            message: format!("unknown interface: {}", name),
        })
}

pub struct Interfaces {
    map: InterfaceMap,
    finalized: bool,
}

struct PeerInterface;

impl PeerInterface {
    fn ping() -> MethodResult {
        Ok(vec![])
    }

    fn get_machine_id() -> MethodResult {
        let mid = format!("{}", MachineId::get());
        Ok(vec![Value::BasicValue(BasicValue::String(mid))])
    }

    pub fn new() -> Interface {
        Interface::new()
            .add_method("Ping", Method::new(|_| Self::ping()))
            .add_method("GetMachineId", Method::new(|_| Self::get_machine_id())
                .add_result(Argument::new("machine_uuid", "s")))
    }
}

struct PropertyInterface;

impl PropertyInterface {
    fn get_property(map: &InterfaceMap, m: &mut Message) -> MethodResult {
        let values = try!(Arguments::new(m));
        let iface = try!(values.extract_string(0));
        let property = try!(values.extract_string(1));

        require_interface(&map.borrow(), iface).and_then(|iface| {
            iface.get_property_value(property)
        })
    }

    fn set_property(map: &mut InterfaceMap, m: &mut Message) -> MethodResult {
        let values = try!(Arguments::new(m));
        let iface = try!(values.extract_string(0));
        let property = try!(values.extract_string(1));
        let value = try!(values.extract(2));

        require_interface(&map.borrow(), iface).and_then(|iface| {
            iface.set_property_value(property, value)
        })
    }

    fn get_all_properties(map: &InterfaceMap, m: &mut Message) -> MethodResult {
        let values = try!(Arguments::new(m));
        let iface = try!(values.extract_string(0));

        require_interface(&map.borrow(), iface).map(|iface| {
            vec![Value::Dictionary(iface.get_property_map())]
        })
    }

    pub fn new(map: InterfaceMap) -> Interface {
        let get_map = map.clone();
        let mut set_map = map.clone();
        let get_all_map = map.clone();

        Interface::new()
            .add_method("Get", Method::new(move |m| Self::get_property(&get_map, m))
                .add_argument(Argument::new("interface_name", "s"))
                .add_argument(Argument::new("property_name", "s"))
                .add_result(Argument::new("value", "v")))
            .add_method("Set", Method::new(move |m| Self::set_property(&mut set_map, m))
                .add_argument(Argument::new("interface_name", "s"))
                .add_argument(Argument::new("property_name", "s"))
                .add_result(Argument::new("value", "v")))
            .add_method("GetAll", Method::new(move |m| Self::get_all_properties(&get_all_map, m))
                .add_argument(Argument::new("interface_name", "s"))
                .add_result(Argument::new("props", "{sv}")))
    }
}

struct IntrospectableInterface;

impl IntrospectableInterface {
    fn introspect(map: &InterfaceMap, children: &ChildrenList, _: &mut Message) -> MethodResult {
        let xml = format!(concat!(
            r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"\n"#,
            r#" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">\n"#,
            r#"<!-- rust-bus {} -->"#,
            r#"<node>\n"#,
            r#"{}"#, // interface
            r#"{}"#, // children
            r#"</node>\n"#),
            env!("CARGO_PKG_VERSION"),
            Self::_to_string_map(&*map.borrow(), |k, v| Self::_introspect_interface(" ", k, v)),
            children.borrow().iter().fold("".to_owned(), |p, name| {
                format!(r#"{} <node name="{}" />"#, p, name)
            }));
        Ok(vec![Value::BasicValue(BasicValue::String(xml))])
    }

    fn _to_string_map<K, V, F>(map: &BTreeMap<K, V>, f: F) -> String
        where F: Fn(&K, &V) -> String {
        map.iter().fold("".to_owned(), |p, (k, v)| {
            format!("{}{}", p, f(k, v))
        })
    }

    fn _to_string_list<T, F>(map: &Vec<T>, f: F) -> String
        where F: Fn(&T) -> String {
        map.iter().fold("".to_owned(), |p, t| {
            format!("{}{}", p, f(t))
        })
    }

    fn _introspect_annotation(indent: &str, ann: &Annotation) -> String {
        format!(r#"{}<annotation name="{}" value="{}" />\n"#,
            indent,
            ann.name,
            ann.value)
    }

    fn _introspect_arg(indent: &str, direction: &str, arg: &Argument) -> String {
        format!(r#"{}<arg name="{}" type="{}" direction="{}" />\n"#,
            indent,
            arg.name,
            arg.signature,
            direction)
    }

    fn _introspect_property(indent: &str, name: &String, prop: &Property) -> String {
        let new_indent = format!("{} ", indent);
        let access =
            match prop.access {
                PropertyAccess::RO(_) => "read",
                PropertyAccess::RW(_) => "readwrite",
                PropertyAccess::WO(_) => "write",
            };
        let sig = match prop.signature { Signature(ref s) => s };
        format!(r#"{}<property name="" type="{}" access="{}">\n{}{}</property>\n"#,
            name,
            sig,
            access,
            Self::_to_string_list(&prop.anns, |t| Self::_introspect_annotation(&new_indent, t)),
            indent)
    }

    fn _introspect_method(indent: &str, name: &String, method: &Method) -> String {
        let new_indent = format!("{} ", indent);
        format!(r#"{}<method name="">\n{}{}{}{}</method>\n"#,
            name,
            Self::_to_string_list(&method.in_args, |t| Self::_introspect_arg(&new_indent, "in", t)),
            Self::_to_string_list(&method.out_args, |t| Self::_introspect_arg(&new_indent, "out", t)),
            Self::_to_string_list(&method.anns, |t| Self::_introspect_annotation(&new_indent, t)),
            indent)
    }

    fn _introspect_signal(indent: &str, name: &String, signal: &Signal) -> String {
        let new_indent = format!("{} ", indent);
        format!(r#"{}<signal name="">\n{}{}{}</signal>\n"#,
            name,
            Self::_to_string_list(&signal.args, |t| Self::_introspect_arg(&new_indent, "out", t)),
            Self::_to_string_list(&signal.anns, |t| Self::_introspect_annotation(&new_indent, t)),
            indent)
    }

    fn _introspect_interface(indent: &str, name: &String, iface: &Interface) -> String {
        let new_indent = format!("{} ", indent);
        format!(r#"{}<interface name="{}">\n{}{}{}{}</interface>\n"#,
            indent,
            name,
            Self::_to_string_map(&iface.properties, |k, v| Self::_introspect_property(&new_indent, k, v)),
            Self::_to_string_map(&iface.methods, |k, v| Self::_introspect_method(&new_indent, k, v)),
            Self::_to_string_map(&iface.signals, |k, v| Self::_introspect_signal(&new_indent, k, v)),
            indent)
    }

    pub fn new(map: InterfaceMap, children: ChildrenList) -> Interface {
        let introspect_map = map.clone();
        let children = children.clone();

        Interface::new()
            .add_method("Introspect", Method::new(move |m| Self::introspect(&introspect_map, &children, m))
                .add_result(Argument::new("xml_data", "s")))
    }
}

impl Interfaces {
    pub fn new() -> Self {
        Interfaces {
            map: Rc::new(RefCell::new(Map::new())),
            finalized: false,
        }
    }

    // Marked as mut for intent; Rc<> doesn't require it though.
    #[allow(unused_mut)]
    pub fn add_interface(mut self, name: &str, iface: Interface) -> Result<Self, Error> {
        if self.finalized {
            return Err(Error::InterfacesFinalized(name.to_owned()));
        }

        {
            let mut map = self.map.borrow_mut();

            match map.entry(name.to_owned()) {
                Entry::Vacant(v)    => {
                    v.insert(iface);

                    Ok(())
                },
                Entry::Occupied(_)  => Err(Error::InterfaceAlreadyRegistered(name.to_owned())),
            }
        }.map(|_| self)
    }

    pub fn finalize(mut self, children: ChildrenList) -> Result<Self, Error> {
        self = try!(Ok(self)
                .and_then(|this| {
                    this.add_interface("org.freedesktop.DBus.Peer", PeerInterface::new())
                }).and_then(|this| {
                    let property_map = this.map.clone();
                    this.add_interface("org.freedesktop.DBus.Properties", PropertyInterface::new(property_map))
                }).and_then(|this| {
                    let introspectable_map = this.map.clone();
                    this.add_interface("org.freedesktop.DBus.Introspectable", IntrospectableInterface::new(introspectable_map, children))
                }));

        self.finalized = true;
        Ok(self)
    }

    pub fn handle(&self, conn: &Connection, msg: &mut Message) -> Option<Result<(), ()>> {
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
