//! A HID report descriptor read into `WebHID`'s `HIDCollectionInfo`s: the
//! collections, their reports and each report's items.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// The collections, and whether input reports start with their id.
pub(crate) struct Parsed {
    pub collections: Vec<Value>,
    pub numbered: bool,
}

#[derive(Clone, Copy, Default)]
struct Globals {
    usage_page: u32,
    report_id: u8,
    report_size: u32,
    report_count: u32,
    logical: (i64, i64),
    physical: (i64, i64),
}

#[derive(Default)]
struct Locals {
    usages: Vec<u32>,
    minimum: Option<u32>,
    maximum: Option<u32>,
}

#[derive(Default)]
struct Collection {
    usage_page: u32,
    usage: u32,
    kind: u32,
    children: Vec<Value>,
    /// Input, output and feature reports, by id.
    reports: [BTreeMap<u8, Vec<Value>>; 3],
}

#[derive(Default)]
struct Reader {
    globals: Globals,
    pushed: Vec<Globals>,
    locals: Locals,
    open: Vec<Collection>,
    done: Vec<Value>,
    numbered: bool,
}

pub(crate) fn parse(descriptor: &[u8]) -> Parsed {
    let mut reader = Reader::default();
    let mut at = 0;
    while let Some(&prefix) = descriptor.get(at) {
        // A long item: its size is in the next byte.
        if prefix == 0xFE {
            let size = descriptor.get(at + 1).copied().unwrap_or_default();
            at += 3 + usize::from(size);
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            size => usize::from(size),
        };
        let data = descriptor.get(at + 1..at + 1 + size).unwrap_or_default();
        reader.item(prefix >> 2 & 3, prefix >> 4, data);
        at += 1 + size;
    }
    while !reader.open.is_empty() {
        reader.end_collection();
    }
    Parsed {
        collections: reader.done,
        numbered: reader.numbered,
    }
}

fn unsigned(data: &[u8]) -> u32 {
    data.iter()
        .rev()
        .fold(0, |value, byte| value << 8 | u32::from(*byte))
}

fn signed(data: &[u8]) -> i64 {
    match *data {
        [a] => i64::from(i8::from_le_bytes([a])),
        [a, b] => i64::from(i16::from_le_bytes([a, b])),
        [a, b, c, d] => i64::from(i32::from_le_bytes([a, b, c, d])),
        _ => i64::from(unsigned(data)),
    }
}

impl Reader {
    fn item(&mut self, kind: u8, tag: u8, data: &[u8]) {
        match kind {
            0 => self.main(tag, data),
            1 => self.global(tag, data),
            2 => self.local(tag, data),
            _ => {}
        }
    }

    fn main(&mut self, tag: u8, data: &[u8]) {
        match tag {
            0xA => self.begin_collection(unsigned(data)),
            0xC => self.end_collection(),
            0x8 => self.report(0, unsigned(data)),
            0x9 => self.report(1, unsigned(data)),
            0xB => self.report(2, unsigned(data)),
            _ => {}
        }
        self.locals = Locals::default();
    }

    fn global(&mut self, tag: u8, data: &[u8]) {
        let globals = &mut self.globals;
        match tag {
            0x0 => globals.usage_page = unsigned(data),
            0x1 => globals.logical.0 = signed(data),
            0x2 => globals.logical.1 = signed(data),
            0x3 => globals.physical.0 = signed(data),
            0x4 => globals.physical.1 = signed(data),
            0x7 => globals.report_size = unsigned(data),
            0x8 => {
                globals.report_id = u8::try_from(unsigned(data)).unwrap_or(u8::MAX);
            }
            0x9 => globals.report_count = unsigned(data),
            _ => self.stack(tag),
        }
    }

    /// Push and pop keep the globals for later.
    fn stack(&mut self, tag: u8) {
        match tag {
            0xA => self.pushed.push(self.globals),
            0xB => self.globals = self.pushed.pop().unwrap_or(self.globals),
            _ => {}
        }
    }

    fn local(&mut self, tag: u8, data: &[u8]) {
        // Four bytes carry their own page.
        let usage = |page: u32| {
            if data.len() == 4 {
                unsigned(data)
            } else {
                page << 16 | unsigned(data)
            }
        };
        let page = self.globals.usage_page;
        match tag {
            0x0 => self.locals.usages.push(usage(page)),
            0x1 => self.locals.minimum = Some(usage(page)),
            0x2 => self.locals.maximum = Some(usage(page)),
            _ => {}
        }
    }

    fn begin_collection(&mut self, kind: u32) {
        let usage = self.locals.usages.first().copied().unwrap_or_default();
        self.open.push(Collection {
            usage_page: usage >> 16,
            usage: usage & 0xFFFF,
            kind,
            ..Collection::default()
        });
    }

    fn end_collection(&mut self) {
        let Some(collection) = self.open.pop() else {
            return;
        };
        let value = collection.into_value();
        match self.open.last_mut() {
            Some(parent) => parent.children.push(value),
            None => self.done.push(value),
        }
    }

    fn report(&mut self, direction: usize, flags: u32) {
        let id = self.globals.report_id;
        if id != 0 && direction == 0 {
            self.numbered = true;
        }
        let item = self.report_item(flags);
        if let Some(collection) = self.open.last_mut() {
            collection.reports[direction]
                .entry(id)
                .or_default()
                .push(item);
        }
    }

    fn report_item(&self, flags: u32) -> Value {
        let Globals {
            report_size,
            report_count,
            logical,
            physical,
            ..
        } = self.globals;
        let flag = |bit: u32| flags & bit != 0;
        let range = self.locals.minimum.zip(self.locals.maximum);
        json!({
            "isConstant": flag(0x1),
            "isArray": !flag(0x2),
            "isAbsolute": !flag(0x4),
            "wrap": flag(0x8),
            "isLinear": !flag(0x10),
            "hasPreferredState": !flag(0x20),
            "hasNull": flag(0x40),
            "isVolatile": flag(0x80),
            "isBufferedBytes": flag(0x100),
            "isRange": range.is_some(),
            "usages": self.locals.usages,
            "usageMinimum": range.map_or(0, |(minimum, _)| minimum),
            "usageMaximum": range.map_or(0, |(_, maximum)| maximum),
            "reportSize": report_size,
            "reportCount": report_count,
            "logicalMinimum": logical.0,
            "logicalMaximum": logical.1,
            "physicalMinimum": physical.0,
            "physicalMaximum": physical.1,
            "unitExponent": 0,
            "unitSystem": "none",
            "strings": [],
        })
    }
}

impl Collection {
    fn into_value(self) -> Value {
        let [input, output, feature] = self.reports.map(|reports| {
            let reports = reports.into_iter();
            reports
                .map(|(id, items)| json!({"reportId": id, "items": items}))
                .collect::<Vec<_>>()
        });
        json!({
            "usagePage": self.usage_page,
            "usage": self.usage,
            "type": self.kind,
            "children": self.children,
            "inputReports": input,
            "outputReports": output,
            "featureReports": feature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vendor device: input report 1 of 8 bytes, output report 2 of 8.
    const VENDOR: [u8; 31] = [
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, // vendor page, usage 1, application
        0x85, 0x01, 0x75, 0x08, 0x95, 0x08, 0x15, 0x00, 0x26, 0xFF, 0x00, // id 1, 8 × 8 bits
        0x09, 0x02, 0x81, 0x02, // input, variable
        0x85, 0x02, 0x09, 0x03, 0x91, 0x02, // id 2, output
        0xC0, 0x00, 0x00,
    ];

    #[test]
    fn collections_and_reports_are_read() {
        let parsed = parse(&VENDOR);
        assert!(parsed.numbered);
        let [collection] = parsed.collections.as_slice() else {
            panic!("one collection");
        };
        assert_eq!(
            (
                collection["usagePage"].as_u64(),
                collection["usage"].as_u64()
            ),
            (Some(0xFF00), Some(1))
        );
        let input = &collection["inputReports"][0];
        assert_eq!(input["reportId"], 1);
        let item = &input["items"][0];
        assert_eq!(
            (item["reportSize"].as_u64(), item["reportCount"].as_u64()),
            (Some(8), Some(8))
        );
        assert_eq!(item["usages"][0].as_u64(), Some(0xFF00_0002));
        assert_eq!(item["logicalMaximum"].as_i64(), Some(255));
        assert_eq!(collection["outputReports"][0]["reportId"], 2);
    }

    #[test]
    fn a_negative_limit_is_read_as_one() {
        assert_eq!(signed(&[0x81]), -127);
        assert_eq!(signed(&[0xFF, 0x00]), 255);
    }
}
