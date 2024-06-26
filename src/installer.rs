use smashline::*;
use smash::lib::lua_const::*;
use smash::app::{lua_bind::*, *};
use smash::hash40;

use crate::{SLOTTED_AGENTS, ACMD_BASE_NAME, StatusScript};

type OpffFunction = unsafe extern "C" fn(&mut L2CFighterCommon);
type OnStartFunction = unsafe extern "C" fn(&mut L2CFighterCommon);

static mut INITIALIZED: [bool; 8] = [false; 8];
static mut SLOTTED_INFO_INDEX: [Option<usize>; 8] = [None; 8];

pub unsafe extern "C" fn on_start(fighter: &mut L2CFighterCommon) {
    INITIALIZED.fill(false);
    SLOTTED_INFO_INDEX.fill(None);

    let slotted_agents = SLOTTED_AGENTS.read();

    if let Some(slotted_info) = slotted_agents.get(&fighter.agent_kind_hash.hash) {
        for (i, info) in slotted_info.iter().enumerate() {
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
    let slotted_agents = SLOTTED_AGENTS.read();

    if !INITIALIZED[entry_id as usize] {
        if let Some(slotted_info) = slotted_agents.get(&fighter.agent_kind_hash.hash) {
            for (i, info) in slotted_info.iter().enumerate() {
                if info.color.contains(&color) {
                    SLOTTED_INFO_INDEX[entry_id as usize] = Some(i);
                }
            }
            if let Some(info_index) = SLOTTED_INFO_INDEX[entry_id as usize] {
                let info = &slotted_info[info_index];
                install_slotted_statuses(fighter, &info.statuses);
                MotionAnimcmdModule::call_script_single(fighter.module_accessor, *FIGHTER_ANIMCMD_GAME, Hash40::new("game_acmd_installer"), -1);
                MotionAnimcmdModule::call_script_single(fighter.module_accessor, *FIGHTER_ANIMCMD_EFFECT, Hash40::new("effect_acmd_installer"), -1);
                MotionAnimcmdModule::call_script_single(fighter.module_accessor, *FIGHTER_ANIMCMD_SOUND, Hash40::new("sound_acmd_installer"), -1);
                MotionAnimcmdModule::call_script_single(fighter.module_accessor, *FIGHTER_ANIMCMD_EXPRESSION, Hash40::new("expression_acmd_installer"), -1);
            }
        }

        INITIALIZED[entry_id as usize] = true;
    }

    if let Some(slotted_info) = slotted_agents.get(&fighter.agent_kind_hash.hash) {
        if let Some(info_index) = SLOTTED_INFO_INDEX[entry_id as usize] {
            let info = &slotted_info[info_index];

            if let Some(opff) = info.frame {
                let f: OpffFunction = std::mem::transmute(opff);
                f(fighter);
            }
        }
    }
}

unsafe fn install_slotted_statuses(fighter: &mut L2CFighterCommon, statuses: &[StatusScript]) {
    for s in statuses {
        fighter.sv_set_status_func(
            s.kind.into(),
            (s.line as i32).into(),
            &mut *(s.function as *mut skyline::libc::c_void)
        );
    }
}

macro_rules! create_acmd_installers {
    ($($category:ident),*) => {
        paste::paste! {
            $(
                pub unsafe extern "C" fn [<$category _acmd_installer>](agent: &mut L2CAgentBase) {
                    let entry_id = WorkModule::get_int(agent.module_accessor, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);
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
                    let entry_id = WorkModule::get_int(agent.module_accessor, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID);
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

pub unsafe extern "C" fn slotted_weapon_installer_pre(weapon: &mut L2CWeaponCommon) -> L2CValue {
//     let category = unsafe { utility::get_category(&mut *weapon.module_accessor) };
//     let kind = unsafe { utility::get_kind(&mut *weapon.module_accessor) };
//     let owner_id = WorkModule::get_int(weapon.module_accessor, *WEAPON_INSTANCE_WORK_ID_INT_LINK_OWNER);
//     let owner_boma = sv_battle_object::module_accessor(owner_id as u32);
//     let owner_color = WorkModule::get_int(owner_boma, *FIGHTER_INSTANCE_WORK_ID_INT_COLOR);
//
//     let slotted_statuses = SLOTTED_STATUSES.read();
//
//     if let Some(statuses) = slotted_statuses.get(&AgentInfo { category, kind }) {
//         let mut found = false;
//
//         for status in statuses {
//             if status.color.contains(&owner_color) {
//                 found = true;
//                 unsafe {
//                     weapon.sv_set_status_func(
//                         status.kind.into(),
//                         (status.line as i32).into(),
//                         &mut *(status.function as *mut skyline::libc::c_void)
//                     );
//                 }
//             }
//         }
//
//         if found {
//             StatusModule::set_status_kind_interrupt(weapon.module_accessor, 0);
//         }
//     }
//
    1.into()
}
