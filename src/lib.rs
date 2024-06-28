#![feature(proc_macro_hygiene)]
#![allow(
    non_snake_case,
    unused
)]

mod util;
mod installer;

use std::collections::{HashMap, HashSet};
use once_cell::sync::Lazy;
use smashline::{*, locks::RwLock};
use smash::hash40;

const UUID_LEN: usize = 8;
pub(crate) static mut UUID: [char; UUID_LEN] = ['\0'; UUID_LEN];

pub(crate) struct AcmdScript {
    category: Acmd,
    function: AcmdFunction,
}

pub(crate) struct StatusScript {
    line: StatusLine,
    kind: i32,
    function: *const (),
}

struct SlottedInfo {
    color: Vec<i32>,
    color_bool: Option<*const [bool; 256]>,
    frame: Option<*const ()>,
    on_start: Option<*const ()>,
    acmds: HashMap<u64, AcmdScript>,
    statuses: Vec<StatusScript>,
}

pub(crate) static SLOTTED_AGENTS: Lazy<RwLock<HashMap<u64, Vec<SlottedInfo>>>> = Lazy::new(|| RwLock::new(HashMap::new()));
static mut INSTALLED_AGENTS: Lazy<HashSet<u64>> = Lazy::new(HashSet::new);
pub(crate) static ACMD_BASE_NAME: Lazy<RwLock<HashMap<u64, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));

pub struct SlottedAgent {
    agent: Agent,
    name: String,
    hash: u64,
    is_weapon: bool,
    is_cloned: bool,
    color: Vec<i32>,
    color_bool: Option<*const [bool; 256]>,
}

impl SlottedAgent {
    pub fn new(agent: &str) -> Self {
        unsafe {
            if UUID.iter().all(|&c| c == '\0') {
                use std::fmt::Write;

                const BUF_LEN: usize = UUID_LEN / 2;
                let mut buf: [u8; BUF_LEN] = [0; BUF_LEN];
                skyline::nn::os::GenerateRandomBytes(buf.as_mut_ptr() as *mut skyline::libc::c_void, BUF_LEN as u64);

                let s = buf
                    .iter()
                    .fold(String::new(), |mut output, b| {
                        let _ = write!(output, "{b:02x}");
                        output
                    });

                for (i, c) in s.chars().enumerate() {
                    UUID[i] = c;
                }
            }
        }

        let fighter_id = util::get_fighter_id(agent);
        let weapon_id = util::get_weapon_id(agent);
        let mut is_weapon = false;

        let hash = if fighter_id != -1 {
            hash40(&("fighter_kind_".to_owned() + agent))
        } else if weapon_id != -1 {
            is_weapon = true;
            hash40(&("weapon_kind_".to_owned() + agent))
        } else {
            hash40("invalid")
        };
        Self {
            agent: Agent::new(agent),
            name: agent.to_string(),
            hash,
            is_weapon,
            is_cloned: false,
            color: Vec::new(),
            color_bool: None,
        }
    }

    pub fn set_color(&mut self, color: &[bool; 256]) -> &mut Self {
        self.color_bool = Some(color as *const [bool; 256]);
        self
    }

    pub fn set_color2(&mut self, color: Vec<i32>) -> &mut Self {
        self.color = color;
        self
    }

    pub fn weapon_cloned_from(&mut self, original_owner: &str, original_name: &str) -> &mut Self {
        let weapon_name = original_owner.to_owned() + "_" + original_name;
        let weapon_id = util::get_weapon_id(weapon_name.as_str());
        
        if weapon_id != -1 {
            self.is_weapon = true;
            self.hash = hash40(&("weapon_kind_".to_owned() + &weapon_name));
            self.is_cloned = true;
        };
        self
    }

    pub fn acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        if name.starts_with("game") {
            self.game_acmd(name, function, priority);
        } else if name.starts_with("effect") {
            self.effect_acmd(name, function, priority);
        } else if name.starts_with("sound") {
            self.sound_acmd(name, function, priority);
        } else if name.starts_with("expression") {
            self.expression_acmd(name, function, priority);
        } else {
            println!("ACMD Category for {} could not be found! Skipping...", name);
        }
        self
    }

    pub fn game_acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.acmd_helper(name, AcmdScript { category: Acmd::Game, function, });
        self
    }

    pub fn effect_acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.acmd_helper(name, AcmdScript { category: Acmd::Effect, function, });
        self
    }

    pub fn sound_acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.acmd_helper(name, AcmdScript { category: Acmd::Sound, function, });
        self
    }

    pub fn expression_acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.acmd_helper(name, AcmdScript { category: Acmd::Expression, function, });
        self
    }

    fn acmd_helper(&mut self, name: &str, script: AcmdScript) {
        let mut slotted_agents = SLOTTED_AGENTS.write();
        let slotted_info = slotted_agents.entry(self.hash).or_default();

        let hash = hash40(name);
        if let Some(info) = slotted_info.iter_mut().find(|info| self.color == info.color) {
            info.acmds.insert(hash, script);
        } else {
            slotted_info.push({
                let mut acmds: HashMap<u64, AcmdScript> = HashMap::new();
                acmds.insert(hash, script);
                SlottedInfo {
                    color: self.color.clone(),
                    color_bool: self.color_bool,
                    frame: None,
                    on_start: None,
                    acmds,
                    statuses: vec![]
                }
            })
        }

        if let Some((_, base_name)) = name.split_once('_') {
            let game = "game_".to_owned() + base_name;
            ACMD_BASE_NAME
                .write()
                .entry(hash40(&game))
                .or_insert_with(|| {
                    base_name.to_string()
                });
        }
    }

    #[allow(unused)]
    pub fn status<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        kind: i32,
        function: M::Function<T>,
    ) -> &mut Self {
        if self.hash == hash40("invalid") {
            println!("Couldn't install slotted status for `{}`", self.name);
            return self;
        }

        let status_script = StatusScript {
            line: M::LINE,
            kind,
            function: unsafe { M::cast_function(function) }
        };

        let mut slotted_agents = SLOTTED_AGENTS.write();
        let slotted_info = slotted_agents.entry(self.hash).or_default();

        if let Some(info) = slotted_info.iter_mut().find(|info| self.color == info.color) {
            info.statuses.push(status_script);
        } else {
            slotted_info.push({
                SlottedInfo {
                    color: self.color.clone(),
                    color_bool: self.color_bool,
                    frame: None,
                    on_start: None,
                    acmds: HashMap::new(),
                    statuses: vec![status_script]
                }
            })
        }
        self
    }

    #[allow(unused)]
    pub fn on_line<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        function: M::LineFunction<T>,
    ) -> &mut Self {
        if self.hash == hash40("invalid") {
            println!("Couldn't install on_line for `{}`", self.name);
            return self;
        }

        let frame = Some(unsafe { M::cast_line_function(function) });

        let mut slotted_agents = SLOTTED_AGENTS.write();
        let slotted_info = slotted_agents.entry(self.hash).or_default();

        if let Some(info) = slotted_info.iter_mut().find(|info| self.color == info.color) {
            info.frame = frame;
        } else {
            slotted_info.push({
                SlottedInfo {
                    color: self.color.clone(),
                    color_bool: self.color_bool,
                    frame,
                    on_start: None,
                    acmds: HashMap::new(),
                    statuses: vec![]
                }
            })
        }
        self
    }

    pub fn on_init<T>(&mut self, func: StateFunction<T>) -> &mut Self {
        self.agent.on_init(func);
        self
    }

    pub fn on_fini<T>(&mut self, func: StateFunction<T>) -> &mut Self {
        self.agent.on_fini(func);
        self
    }

    pub fn on_start<T>(&mut self, func: StateFunction<T>) -> &mut Self {
        if self.hash == hash40("invalid") {
            println!("Couldn't install on_start for `{}`", self.name);
            return self;
        }

        let f = Some(func as *const ());

        let mut slotted_agents = SLOTTED_AGENTS.write();
        let slotted_info = slotted_agents.entry(self.hash).or_default();

        if let Some(info) = slotted_info.iter_mut().find(|info| self.color == info.color) {
            info.on_start = f;
        } else {
            slotted_info.push({
                SlottedInfo {
                    color: self.color.clone(),
                    color_bool: self.color_bool,
                    frame: None,
                    on_start: f,
                    acmds: HashMap::new(),
                    statuses: vec![]
                }
            })
        }
        self
    }

    pub fn on_end<T>(&mut self, func: StateFunction<T>) -> &mut Self {
        self.agent.on_end(func);
        self
    }

    pub fn install(&mut self) {
        unsafe {
            if INSTALLED_AGENTS.contains(&self.hash) {
                return;
            }
        }

        if self.is_weapon {
            let f = if self.is_cloned {
                installer::slotted_cloned_weapon_installer_pre
            } else {
                installer::slotted_weapon_installer_pre
            };
            self.agent.status(Pre, 0, f);

            if let Some(slotted_info) = SLOTTED_AGENTS.read().get(&self.hash) {
                if let Some(info) = slotted_info.iter().find(|info| self.color == info.color) {
                    if self.is_cloned {
                        info.statuses
                            .iter()
                            .filter(|status| status.kind != 0)
                            .for_each(|status| {
                                self.agent.status(Pre, status.kind, f);
                            });
                    }

                    if info.frame.is_some() {
                        self.agent.on_line(Main, installer::weapon_opff);
                    }
                }
            }
        } else {
            self.agent.on_start(installer::on_start);
            self.agent.on_line(Main, installer::opff);
        }

        let uuid: String = unsafe { UUID.iter().collect() };
        self.agent.acmd(&format!("game_acmd_installer{}", uuid), installer::game_acmd_installer, Priority::Default);
        self.agent.acmd(&format!("effect_acmd_installer{}", uuid), installer::effect_acmd_installer, Priority::Default);
        self.agent.acmd(&format!("sound_acmd_installer{}", uuid), installer::sound_acmd_installer, Priority::Default);
        self.agent.acmd(&format!("expression_acmd_installer{}", uuid), installer::expression_acmd_installer, Priority::Default);

        self.agent.install();

        unsafe {
            INSTALLED_AGENTS.insert(self.hash);
        }
    }
}

pub fn get_colors(agent: &mut L2CAgentBase) -> Vec<i32> {
    use smash::lib::lua_const::*;
    use smash::app::{lua_bind::*, *};

    let category = utility::get_category(unsafe { &mut *agent.module_accessor });
    let boma = if category == *BATTLE_OBJECT_CATEGORY_FIGHTER {
        agent.module_accessor
    } else {
        let owner_id = unsafe { WorkModule::get_int(agent.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER) };
        unsafe { sv_battle_object::module_accessor(owner_id as u32) }
    };
    let color = unsafe { WorkModule::get_int(boma, *FIGHTER_INSTANCE_WORK_ID_INT_COLOR) };

    if let Some(slotted_info) = SLOTTED_AGENTS.read().get(&agent.agent_kind_hash.hash) {
        for (i, info) in slotted_info.iter().enumerate() {
            if info.color.contains(&color) {
                return info.color.clone();
            }
        }
    }
    Vec::new()
}
