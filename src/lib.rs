#![feature(proc_macro_hygiene)]
#![allow(
    non_snake_case,
    unused
)]

mod util;

use std::collections::BTreeMap;
use smashline::{*, locks::RwLock};
use smash::lib::lua_const::*;
use smash::app::{lua_bind::*, *};

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct AgentInfo {
    category: i32,
    kind: i32,
}

struct StatusScript {
    line: StatusLine,
    kind: i32,
    function: *const (),
    color: Vec<i32>,
}

static SLOTTED_STATUSES: RwLock<BTreeMap<AgentInfo,Vec<StatusScript>>> = RwLock::new(BTreeMap::new());

pub struct SlottedAgent {
    agent: Agent,
    name: String,
    weapon_name: String,
    color: Vec<i32>,
}

impl SlottedAgent {
    pub fn new(agent: &str, original_name: &str, color: &[bool]) -> Self {
        let mut c: Vec<i32> = Vec::new();
        for (i, &v) in color.iter().enumerate() {
            if v {
                c.push(i as i32);
            }
        }
        Self {
            agent: Agent::new(agent),
            name: agent.to_string(),
            weapon_name: String::new(),
            color: c
        }
    }

    pub fn new2(agent: &str, original_name: &str, color: Vec<i32>) -> Self {
        Self {
            agent: Agent::new(agent),
            name: agent.to_string(),
            weapon_name: original_name.to_string(),
            color
        }
    }

    pub fn acmd(&mut self, name: &str, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.agent.acmd(name, function, priority);
        self
    }

    pub fn game_acmd(&mut self, name: impl AsHash40, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.agent.game_acmd(name, function, priority);
        self
    }

    pub fn effect_acmd(&mut self, name: impl AsHash40, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.agent.effect_acmd(name, function, priority);
        self
    }

    pub fn sound_acmd(&mut self, name: impl AsHash40, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.agent.sound_acmd(name, function, priority);
        self
    }

    pub fn expression_acmd(&mut self, name: impl AsHash40, function: AcmdFunction, priority: Priority) -> &mut Self {
        self.agent.expression_acmd(name, function, priority);
        self
    }

    #[allow(unused)]
    /// Regular status install
    pub fn status<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        kind: i32,
        function: M::Function<T>,
    ) -> &mut Self {
        self.agent.status(line, kind, function);
        self
    }

    #[allow(unused)]
    /// Slotted status install
    /// Adds statuses to a "queue", and can be installed using `install_slotted_statuses`
    pub fn status2<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        kind: i32,
        function: M::Function<T>,
    ) -> &mut Self {
        if self.weapon_name.is_empty() {
            self.status_f(line, kind, function);
        } else {
            self.status_w(line, kind, function);
        }
        self
    }

    #[allow(unused)]
    fn status_f<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        kind: i32,
        function: M::Function<T>,
    ) -> &mut Self {
        let fighter_id = util::get_fighter_id(&self.name);
        if fighter_id != -1 {
            SLOTTED_STATUSES
                .write()
                .entry(AgentInfo { category: *BATTLE_OBJECT_CATEGORY_FIGHTER, kind: fighter_id })
                .or_default()
                .push(StatusScript {
                    line: M::LINE,
                    kind,
                    function: unsafe { M::cast_function(function) },
                    color: self.color.clone()
                });
        } else {
            println!("Couldn't install slotted status for `{}`", self.name);
        }
        self
    }

    #[allow(unused)]
    fn status_w<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        kind: i32,
        function: M::Function<T>,
    ) -> &mut Self {
        let weapon_id = util::get_weapon_id(&self.weapon_name);
        if weapon_id != -1 {
            SLOTTED_STATUSES
                .write()
                .entry(AgentInfo { category: *BATTLE_OBJECT_CATEGORY_WEAPON, kind: weapon_id })
                .or_default()
                .push(StatusScript {
                    line: M::LINE,
                    kind,
                    function: unsafe { M::cast_function(function) },
                    color: self.color.clone()
                });
        } else {
            println!("Couldn't install slotted status for `{} ({})`", self.name, self.weapon_name);
        }
        self
    }

    #[allow(unused)]
    pub fn on_line<M: StatusLineMarker, T>(
        &mut self,
        line: M,
        function: M::LineFunction<T>,
    ) -> &mut Self {
        self.agent.on_line(line, function);
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
        self.agent.on_start(func);
        self
    }

    pub fn on_end<T>(&mut self, func: StateFunction<T>) -> &mut Self {
        self.agent.on_end(func);
        self
    }

    pub fn install(&mut self) {
        if !self.weapon_name.is_empty() {
            self.agent.status(Pre, 0, slotted_weapon_installer_pre);
        }
        self.agent.install();
    }
}

pub fn install_slotted_statuses(agent: &mut L2CFighterBase) {
    let category = unsafe { utility::get_category(&mut *agent.module_accessor) };
    let kind = unsafe { utility::get_kind(&mut *agent.module_accessor) };
    let slotted_statuses = SLOTTED_STATUSES.read();

    if let Some(statuses) = slotted_statuses.get(&AgentInfo { category, kind }) {
        for status in statuses {
            unsafe {
                agent.sv_set_status_func(
                    status.kind.into(),
                    (status.line as i32).into(),
                    &mut *(status.function as *mut skyline::libc::c_void)
                );
            }
        }
    }
}

unsafe extern "C" fn slotted_weapon_installer_pre(weapon: &mut L2CWeaponCommon) -> L2CValue {
    let category = unsafe { utility::get_category(&mut *weapon.module_accessor) };
    let kind = unsafe { utility::get_kind(&mut *weapon.module_accessor) };
    let owner_id = WorkModule::get_int(weapon.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
    let owner_boma = sv_battle_object::module_accessor(owner_id as u32);
    let owner_color = WorkModule::get_int(owner_boma, *FIGHTER_INSTANCE_WORK_ID_INT_COLOR);

    let slotted_statuses = SLOTTED_STATUSES.read();

    if let Some(statuses) = slotted_statuses.get(&AgentInfo { category, kind }) {
        let mut found = false;

        for status in statuses {
            if status.color.contains(&owner_color) {
                found = true;
                unsafe {
                    weapon.sv_set_status_func(
                        status.kind.into(),
                        (status.line as i32).into(),
                        &mut *(status.function as *mut skyline::libc::c_void)
                    );
                }
            }
        }

        if found {
            StatusModule::set_status_kind_interrupt(weapon.module_accessor, 0);
        }
    }

    1.into()
}
