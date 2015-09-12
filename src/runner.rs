extern crate dbus;
use self::dbus::ConnectionItem;

use super::connection::{DBusBusType, DBusConnection};
use super::error::DBusError;
use super::server::DBusServer;

use std::collections::btree_map::{BTreeMap, Entry};

pub struct DBusRunner<'a> {
    conn: DBusConnection,

    servers: BTreeMap<String, DBusServer<'a>>,
}

impl<'a> DBusRunner<'a> {
    pub fn new(bus: DBusBusType) -> Result<DBusRunner<'a>, DBusError> {
        let conn = try!(DBusConnection::get_private(bus));

        Ok(DBusRunner {
            conn: conn,

            servers: BTreeMap::new(),
        })
    }

    pub fn add_server(&'a mut self, name: &str) -> Result<&'a mut DBusServer<'a>, DBusError> {
        match self.servers.entry(name.to_owned()) {
            Entry::Vacant(v)    => {
                let server = try!(DBusServer::new(&self.conn, name));

                Ok(v.insert(server))
            },
            Entry::Occupied(_)  => Err(DBusError::ServerAlreadyRegistered(name.to_owned())),
        }
    }

    pub fn remove_server(&mut self, name: &str) -> Result<&mut Self, DBusError> {
        match self.servers.remove(name) {
            Some(_) => Ok(self),
            None    => Err(DBusError::NoSuchServer(name.to_owned())),
        }
    }

    pub fn run(&mut self, timeout: i32) -> () {
        let servers = &mut self.servers;

        self.conn.iter(timeout).fold((), |_, item| {
            match item {
                ConnectionItem::MethodCall(m) => Some(m),
                ConnectionItem::Signal(s)     => Some(s),
                ConnectionItem::Nothing       => None,
            }.as_mut().map(|m| {
                servers.iter_mut().fold(Some(m), |opt_m, (_, server)| {
                    opt_m.and_then(|m| {
                        server.handle_message(m)
                    })
                })
            });
        });
    }
}
