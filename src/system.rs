// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Game-wide settings in `lib/system` (`olympia/io.c:load_system`,
//! `save_system`).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::cstr::{atoi, scan_ints};
use crate::entity::{OlyTime, Text};
use crate::lines::getlin_ew_lines;
use crate::os::strerror;

/// The globals `lib/system` sets, with the C engine's initial values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    pub sysclock: OlyTime,
    pub from_host: Option<Text>,
    pub reply_host: Option<Text>,
    pub gm_address: Option<Text>,
    pub game_title: Option<Text>,
    pub game_url: Option<Text>,
    pub rules_url: Option<Text>,
    pub times_url: Option<Text>,
    pub htpasswd_loc: Option<Text>,
    pub garrison_pay: i32,
    pub auto_quit_turns: i32,
    pub army_slow_factor: i32,
    pub indep_player: i32,
    pub gm_player: i32,
    pub skill_player: i32,
    pub post_has_been_run: i32,
    pub seed_has_been_run: i32,
    pub faery_region: i32,
    pub tunnel_region: i32,
    pub under_region: i32,
    pub faery_player: i32,
    pub hades_region: i32,
    pub hades_pit: i32,
    pub hades_player: i32,
    pub nowhere_region: i32,
    pub nowhere_loc: i32,
    pub npc_pl: i32,
    pub cloud_region: i32,
    pub combat_pl: i32,
    pub mount_olympus: i32,
}

impl Default for System {
    fn default() -> Self {
        let text = |s: &str| Some(s.as_bytes().to_vec());
        Self {
            sysclock: OlyTime::default(),
            from_host: text("g3@olympia.v-labs.be"),
            reply_host: text("g3@olympia.v-labs.be"),
            gm_address: text("c@dvdc.be"),
            game_title: text("Olympia G3"),
            game_url: text("http://olympia.v-labs.be/g3"),
            rules_url: text("http://oly.v-labs.be/g3/node/11"),
            times_url: text("http://oly.v-labs.be/g3/node/409"),
            htpasswd_loc: text("/u/oly/g2/lib/ht-passwords"),
            garrison_pay: 2,
            auto_quit_turns: 0,
            army_slow_factor: 20,
            indep_player: 100,
            gm_player: 200,
            skill_player: 202,
            post_has_been_run: 0,
            seed_has_been_run: 0,
            faery_region: 0,
            tunnel_region: 0,
            under_region: 0,
            faery_player: 0,
            hades_region: 0,
            hades_pit: 0,
            hades_player: 0,
            nowhere_region: 0,
            nowhere_loc: 0,
            npc_pl: 206,
            cloud_region: 0,
            combat_pl: 0,
            mount_olympus: 0,
        }
    }
}

/// `olytime_scan()`: `turn day days_since_epoch`, keeping any field the
/// text doesn't reach.
pub fn olytime_scan(s: &[u8], t: &mut OlyTime) {
    let v = scan_ints(s, 3);
    if let Some(&turn) = v.first() {
        t.turn = turn as i16;
    }
    if let Some(&day) = v.get(1) {
        t.day = day as i16;
    }
    if let Some(&days) = v.get(2) {
        t.days_since_epoch = days;
    }
}

/// `olytime_print()`: the header and date, unless the date is all zero.
pub fn olytime_print(out: &mut Vec<u8>, header: &str, t: &OlyTime) {
    if *t != OlyTime::default() {
        out.extend_from_slice(
            format!("{header}{} {} {}\n", t.turn, t.day, t.days_since_epoch).as_bytes(),
        );
    }
}

impl System {
    /// Reads `lib/system` over the current values (`load_system()`).
    pub fn load(&mut self, path: &Path) {
        let lines = match getlin_ew_lines(path) {
            Ok(lines) => lines,
            Err(err) => {
                eprintln!(
                    "load_system: can't read {}: {}",
                    path.display(),
                    strerror(&err)
                );
                return;
            }
        };
        for s in lines {
            if s.is_empty() || s[0] == b'#' {
                continue;
            }
            if let Some(rest) = s.strip_prefix(b"sysclock:") {
                olytime_scan(rest, &mut self.sysclock);
            } else if let Some((field, value)) = self.text_field(&s) {
                *field = Some(value);
            } else if let Some((field, value)) = self.int_field(&s) {
                *field = value;
            } else {
                eprintln!(
                    "load_system: unrecognized line: {}",
                    String::from_utf8_lossy(&s)
                );
            }
        }
    }

    /// The string setting a `key=value` line names, and its value.
    fn text_field(&mut self, s: &[u8]) -> Option<(&mut Option<Text>, Text)> {
        let fields: [(&[u8], &mut Option<Text>); 8] = [
            (b"from_host=", &mut self.from_host),
            (b"reply_host=", &mut self.reply_host),
            (b"gm_address=", &mut self.gm_address),
            (b"game_title=", &mut self.game_title),
            (b"game_url=", &mut self.game_url),
            (b"rules_url=", &mut self.rules_url),
            (b"times_url=", &mut self.times_url),
            (b"htpasswd_loc=", &mut self.htpasswd_loc),
        ];
        fields
            .into_iter()
            .find_map(|(key, field)| s.strip_prefix(key).map(|value| (field, value.to_vec())))
    }

    /// The integer setting a `key=value` line names, and its value.
    fn int_field(&mut self, s: &[u8]) -> Option<(&mut i32, i32)> {
        let fields: [(&[u8], &mut i32); 21] = [
            (b"garrison_pay=", &mut self.garrison_pay),
            (b"auto_quit=", &mut self.auto_quit_turns),
            (b"army_slow_factor=", &mut self.army_slow_factor),
            (b"indep_player=", &mut self.indep_player),
            (b"gm_player=", &mut self.gm_player),
            (b"skill_player=", &mut self.skill_player),
            (b"post=", &mut self.post_has_been_run),
            (b"init=", &mut self.seed_has_been_run),
            (b"fr=", &mut self.faery_region),
            (b"tr=", &mut self.tunnel_region),
            (b"ur=", &mut self.under_region),
            (b"fp=", &mut self.faery_player),
            (b"hr=", &mut self.hades_region),
            (b"hp=", &mut self.hades_pit),
            (b"hl=", &mut self.hades_player),
            (b"nr=", &mut self.nowhere_region),
            (b"nl=", &mut self.nowhere_loc),
            (b"np=", &mut self.npc_pl),
            (b"cr=", &mut self.cloud_region),
            (b"cp=", &mut self.combat_pl),
            (b"mo=", &mut self.mount_olympus),
        ];
        fields
            .into_iter()
            .find_map(|(key, field)| s.strip_prefix(key).map(|value| (field, atoi(value))))
    }

    /// Writes `lib/system` (`save_system()`).
    pub fn save(&self, path: &Path) {
        let mut out = Vec::new();
        olytime_print(&mut out, "sysclock: ", &self.sysclock);
        let int = |out: &mut Vec<u8>, key: &str, v: i32| {
            out.extend_from_slice(format!("{key}={v}\n").as_bytes());
        };
        let text = |out: &mut Vec<u8>, key: &str, v: &Option<Text>| {
            out.extend_from_slice(key.as_bytes());
            out.push(b'=');
            // printf("%s", NULL) prints "(null)".
            out.extend_from_slice(v.as_deref().unwrap_or(b"(null)"));
            out.push(b'\n');
        };
        int(&mut out, "indep_player", self.indep_player);
        int(&mut out, "gm_player", self.gm_player);
        int(&mut out, "skill_player", self.skill_player);
        text(&mut out, "from_host", &self.from_host);
        text(&mut out, "reply_host", &self.reply_host);
        text(&mut out, "gm_address", &self.gm_address);
        text(&mut out, "game_title", &self.game_title);
        text(&mut out, "game_url", &self.game_url);
        text(&mut out, "rules_url", &self.rules_url);
        text(&mut out, "times_url", &self.times_url);
        text(&mut out, "htpasswd_loc", &self.htpasswd_loc);
        int(&mut out, "garrison_pay", self.garrison_pay);
        if self.auto_quit_turns > 0 {
            int(&mut out, "auto_quit", self.auto_quit_turns);
        }
        int(&mut out, "army_slow_factor", self.army_slow_factor);
        int(&mut out, "post", self.post_has_been_run);
        int(&mut out, "init", self.seed_has_been_run);
        int(&mut out, "fr", self.faery_region);
        int(&mut out, "tr", self.tunnel_region);
        int(&mut out, "ur", self.under_region);
        int(&mut out, "fp", self.faery_player);
        int(&mut out, "hr", self.hades_region);
        int(&mut out, "hp", self.hades_pit);
        int(&mut out, "hl", self.hades_player);
        int(&mut out, "nr", self.nowhere_region);
        int(&mut out, "nl", self.nowhere_loc);
        int(&mut out, "np", self.npc_pl);
        int(&mut out, "cr", self.cloud_region);
        int(&mut out, "cp", self.combat_pl);
        if self.mount_olympus > 0 {
            int(&mut out, "mo", self.mount_olympus);
        }

        match File::create(path) {
            // fprintf() is unchecked in C.
            Ok(mut file) => {
                let _ = file.write_all(&out);
            }
            Err(err) => eprintln!(
                "load_system: can't write {}: {}",
                path.display(),
                strerror(&err)
            ),
        }
    }
}
