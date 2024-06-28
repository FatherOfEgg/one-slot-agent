use smashline::*;
use smash::lib::lua_const::*;
use smash::app::{lua_bind::*, *};
use smash::hash40;

use crate::{SLOTTED_AGENTS, ACMD_BASE_NAME, StatusScript, UUID};

type OpffFunction = unsafe extern "C" fn(&mut L2CFighterCommon);
type OnStartFunction = unsafe extern "C" fn(&mut L2CFighterCommon);

static mut INITIALIZED: [bool; 8] = [false; 8];
static mut SLOTTED_INFO_INDEX: [Option<usize>; 8] = [None; 8];
static mut OPFF: [Option<OpffFunction>; 8] = [None; 8];

static mut COLOR_BOOL_CONVERTED: bool = false;

pub unsafe extern "C" fn on_start(fighter: &mut L2CFighterCommon) {
    INITIALIZED.fill(false);
    SLOTTED_INFO_INDEX.fill(None);
    OPFF.fill(None);

    if !COLOR_BOOL_CONVERTED {
        let mut slotted_agents = SLOTTED_AGENTS.write();

        if let Some(slotted_info) = slotted_agents.get_mut(&fighter.agent_kind_hash.hash) {
            for info in slotted_info.iter_mut() {
                if let Some(c) = info.color_bool {
                    if info.color.is_empty() {
                        info.color = (*c).iter()
                            .enumerate()
                            .filter_map(|(i, &v)| if v { Some(i as i32) } else { None })
                            .collect();
                        info.color_bool = None;
                    }
                }
            }
        }

        COLOR_BOOL_CONVERTED = true;
    }

    let slotted_agents = SLOTTED_AGENTS.read();

    if let Some(slotted_info) = slotted_agents.get(&fighter.agent_kind_hash.hash) {
        for info in slotted_info.iter() {
            if let Some(on_start) = info.on_start {
                let f: OnStartFunction = std::mem::transmute(on_start);
                f(fighter);
            }
        }
    }
}

pub unsafe extern "C" fn opff(fighter: &mut L2CFighterCommon) {
    let entry_id = WorkModule::get_int(fighter.module_accessor, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);
    let color = WorkModule::get_int(fighter.module_accessor, *FIGHTER_INSTANCE_WORK_ID_INT_COLOR);

    if !INITIALIZED[entry_id as usize] {
        let slotted_agents = SLOTTED_AGENTS.read();

        if let Some(slotted_info) = slotted_agents.get(&fighter.agent_kind_hash.hash) {
            for (i, info) in slotted_info.iter().enumerate() {
                if info.color.contains(&color) {
                    SLOTTED_INFO_INDEX[entry_id as usize] = Some(i);

                    install_slotted_acmds(fighter);
                    install_slotted_statuses(fighter, &info.statuses);

                    if let Some(opff) = info.frame {
                        let f: OpffFunction = std::mem::transmute(opff);
                        OPFF[entry_id as usize] = Some(f);
                    }
                }
            }
        }

        INITIALIZED[entry_id as usize] = true;
    }

    if let Some(f) = OPFF[entry_id as usize] {
        f(fighter);
    }
}

pub unsafe extern "C" fn weapon_opff(weapon: &mut L2CFighterCommon) {
    let owner_id = WorkModule::get_int(weapon.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
    let owner_boma = sv_battle_object::module_accessor(owner_id as u32);
    let owner_entry_id = WorkModule::get_int(owner_boma, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);

    let slotted_agents = SLOTTED_AGENTS.read();

    if let Some(slotted_info) = slotted_agents.get(&weapon.agent_kind_hash.hash) {
        if let Some(info_index) = SLOTTED_INFO_INDEX[owner_entry_id as usize] {
            let info = &slotted_info[info_index];

            if let Some(opff) = info.frame {
                let f: OpffFunction = std::mem::transmute(opff);
                f(weapon);
            }
        }
    }
}

unsafe fn install_slotted_acmds(agent: &mut L2CFighterBase) {
    let category = utility::get_category(&mut *agent.module_accessor);
    let uuid: String = unsafe { UUID.iter().collect() };

    if category == *BATTLE_OBJECT_CATEGORY_FIGHTER {
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *FIGHTER_ANIMCMD_GAME, Hash40::new(&format!("game_acmd_installer{}", uuid)), -1);
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *FIGHTER_ANIMCMD_EFFECT, Hash40::new(&format!("effect_acmd_installer{}", uuid)), -1);
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *FIGHTER_ANIMCMD_SOUND, Hash40::new(&format!("sound_acmd_installer{}", uuid)), -1);
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *FIGHTER_ANIMCMD_EXPRESSION, Hash40::new(&format!("expression_acmd_installer{}", uuid)), -1);
    } else {
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *WEAPON_ANIMCMD_GAME, Hash40::new(&format!("game_acmd_installer{}", uuid)), -1);
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *WEAPON_ANIMCMD_EFFECT, Hash40::new(&format!("effect_acmd_installer{}", uuid)), -1);
        MotionAnimcmdModule::call_script_single(agent.module_accessor, *WEAPON_ANIMCMD_SOUND, Hash40::new(&format!("sound_acmd_installer{}", uuid)), -1);
    }
}

unsafe fn install_slotted_statuses(agent: &mut L2CFighterBase, statuses: &[StatusScript]) -> bool {
    let mut restore_original = true;
    for s in statuses {
        if s.kind == 0 && s.line == StatusLine::Pre {
            restore_original = false;
        }
        agent.sv_set_status_func(
            s.kind.into(),
            (s.line as i32).into(),
            &mut *(s.function as *mut skyline::libc::c_void)
        );
    }
    restore_original
}

macro_rules! create_acmd_installers {
    ($($category:ident),*) => {
        paste::paste! {
            $(
                pub unsafe extern "C" fn [<$category _acmd_installer>](agent: &mut L2CAgentBase) {
                    let category = utility::get_category(&mut *agent.module_accessor);
                    let boma = if category == *BATTLE_OBJECT_CATEGORY_FIGHTER {
                        agent.module_accessor
                    } else {
                        let owner_id = WorkModule::get_int(agent.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
                        sv_battle_object::module_accessor(owner_id as u32)
                    };
                    let entry_id = WorkModule::get_int(boma, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);
                    let slotted_agents = SLOTTED_AGENTS.read();

                    if let Some(slotted_info) = slotted_agents.get(&agent.agent_kind_hash.hash) {
                        if let Some(info_index) = SLOTTED_INFO_INDEX[entry_id as usize] {
                            let info = &slotted_info[info_index];
                            let acmds = &info.acmds;

                            for (hash, script) in acmds {
                                if script.category as i32 == Acmd::[<$category:camel>] as i32 {
                                    agent.sv_set_function_hash(
                                        std::mem::transmute([<$category _hub>] as *const ()),
                                        Hash40::new_raw(*hash)
                                    );
                                }
                            }
                        }
                    }
                }
            )*
        }
    };
}

create_acmd_installers!(game, effect, sound, expression);

macro_rules! create_acmd_hubs {
    ($($category:ident),*) => {
        paste::paste! {
            $(
                unsafe extern "C" fn [<$category _hub>](agent: &mut L2CAgentBase, _variadic: &mut Variadic) -> u64 {
                    let category = utility::get_category(&mut *agent.module_accessor);
                    let boma = if category == *BATTLE_OBJECT_CATEGORY_FIGHTER {
                        agent.module_accessor
                    } else {
                        let owner_id = WorkModule::get_int(agent.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
                        sv_battle_object::module_accessor(owner_id as u32)
                    };
                    let entry_id = WorkModule::get_int(boma, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);
                    let slotted_agents = SLOTTED_AGENTS.read();

                    if let Some(slotted_info) = slotted_agents.get(&agent.agent_kind_hash.hash) {
                        if let Some(info_index) = SLOTTED_INFO_INDEX[entry_id as usize] {
                            let info = &slotted_info[info_index];
                            let acmds = &info.acmds;

                            let motion_kind = MotionModule::motion_kind(agent.module_accessor);
                            let game_hash = MotionModule::animcmd_name_hash(agent.module_accessor, Hash40::new_raw(motion_kind));

                            if let Some(base_name) = ACMD_BASE_NAME.read().get(&game_hash) {
                                let script_name = stringify!([<$category>]).to_string() + "_" + base_name;
                                if let Some(script) = acmds.get(&hash40(&script_name)) {
                                    (script.function)(agent);
                                }
                            }
                        }
                    }

                    0
                }
            )*
        }
    };
}

create_acmd_hubs!(game, effect, sound, expression);

pub unsafe extern "C" fn weapon_installer_helper(weapon: &mut L2CWeaponCommon) -> bool {
    let owner_id = WorkModule::get_int(weapon.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
    let owner_boma = sv_battle_object::module_accessor(owner_id as u32);
    let owner_entry_id = WorkModule::get_int(owner_boma, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);

    let slotted_agents = SLOTTED_AGENTS.read();

    install_slotted_acmds(weapon);

    let mut restore_original = true;

    if let Some(slotted_info) = slotted_agents.get(&weapon.agent_kind_hash.hash) {
        if let Some(info_index) = SLOTTED_INFO_INDEX[owner_entry_id as usize] {
            let info = &slotted_info[info_index];
            if !install_slotted_statuses(weapon, &info.statuses) {
                restore_original = false;
            }
        }
    }

    restore_original
}

pub unsafe extern "C" fn slotted_weapon_installer_pre(weapon: &mut L2CWeaponCommon) -> L2CValue {
    let ret = original_status(Pre, weapon, 0);
    if weapon_installer_helper(weapon) {
        weapon.sv_set_status_func(
            0.into(),
            LUA_SCRIPT_STATUS_FUNC_STATUS_PRE.into(),
            &mut *(ret as *const () as *mut skyline::libc::c_void)
        );
    }
    ret(weapon)
}

pub unsafe extern "C" fn slotted_cloned_weapon_installer_pre(weapon: &mut L2CWeaponCommon) -> L2CValue {
    weapon_installer_helper(weapon);
    StatusModule::set_status_kind_interrupt(weapon.module_accessor, 0);
    1.into()
}
