// Copyright 2011 the V8 project authors. All rights reserved.
// Derived from Node v24.2.0 V8 date parser. See LICENSE-V8 and provenance.json.
#[derive(Default)]
pub(super) struct State {
    pub(super) day: Vec<i32>,
    pub(super) time: Vec<i32>,
    pub(super) month: Option<i32>,
    pub(super) iso: bool,
    pub(super) hour_offset: Option<i32>,
    pub(super) sign: Option<i32>,
    pub(super) zone_hour: Option<i32>,
    pub(super) zone_min: Option<i32>,
}
impl State {
    pub(super) fn day_add(&mut self, n: i32) -> bool {
        if self.day.len() == 3 {
            false
        } else {
            self.day.push(n);
            true
        }
    }
    pub(super) fn time_add(&mut self, n: i32) -> bool {
        if self.time.len() == 4 {
            false
        } else {
            self.time.push(n);
            true
        }
    }
    pub(super) fn time_final(&mut self, n: i32) -> bool {
        if !self.time_add(n) {
            return false;
        }
        self.time.resize(4, 0);
        true
    }
    pub(super) fn expecting(&self, n: i32) -> bool {
        match self.time.len() {
            1 | 2 => (0..=59).contains(&n),
            3 => (0..=999).contains(&n),
            _ => false,
        }
    }
    pub(super) fn set_zone(&mut self, n: i32) {
        self.sign = Some(if n < 0 { -1 } else { 1 });
        self.zone_hour = Some(n.abs());
        self.zone_min = Some(0);
    }
}
