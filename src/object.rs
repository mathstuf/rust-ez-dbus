use super::interface::DBusInterface;

extern crate dbus;
use self::dbus::{Connection, Error};

pub struct DBusObject<'a> {
    ifaces: Vec<DBusInterface>,

    conn: &'a Connection,
    path: String,
}

impl<'a> DBusObject<'a> {
    pub fn new(conn: &'a Connection, ifaces: Vec<DBusInterface>, path: &str) -> Result<Self, Error> {
        try!(conn.register_object_path(path));

        Ok(DBusObject {
            ifaces: ifaces,

            conn: conn,
            path: path.to_string(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path[..]
    }
}

impl<'a> Drop for DBusObject<'a> {
    fn drop(&mut self) {
        self.conn.unregister_object_path(self.path())
    }
}
