use maxminddb::{Reader, geoip2};
use std::net::IpAddr;

pub struct Geo {
    reader: Reader<Vec<u8>>,
}

impl Geo {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        Ok(Self {
            reader: Reader::open_readfile(path)?,
        })
    }

    pub fn country(&self, ip: IpAddr) -> Option<String> {
        let lookup = self.reader.lookup(ip).ok()?;
        let record: geoip2::Country = lookup.decode().ok()??;
        record.country.iso_code.map(str::to_string)
    }
}
