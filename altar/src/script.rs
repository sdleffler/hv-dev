use std::io::Read;

use hv::{
    ecs::{Entity, QueryMarker, SystemContext, Without},
    fs::Filesystem,
    prelude::*,
    sync::elastic::Elastic,
};

#[derive(Debug, Clone)]
pub struct Script {
    pub path: String,
}

pub struct ScriptContext {
    // Environment table for scripts loaded in this context
    env: LuaRegistryKey,
    stretched_fs: Elastic<&'static mut Filesystem>,
    // Queued entities to have their newly loaded scripts attached.
    queued: Vec<(Entity, LuaRegistryKey)>,
}

pub fn load_new_scripts(
    context: SystemContext,
    (lua, script_context, fs): (&Lua, &mut ScriptContext, &mut Filesystem),
    (without_registry_key,): (QueryMarker<Without<LuaRegistryKey, &Script>>,),
) {
    let fs_guard = script_context.stretched_fs.loan(fs);
    let mut buf = String::new();
    let env_table: LuaTable = lua.registry_value(&script_context.env).unwrap();

    for (entity, script) in context.query(without_registry_key).iter() {
        let mut mut_fs = script_context.stretched_fs.borrow_mut().unwrap();
        buf.clear();
        mut_fs
            .open(&script.path)
            .unwrap()
            .read_to_string(&mut buf)
            .unwrap();
        drop(mut_fs);
        let loaded: LuaValue = lua
            .load(&buf)
            .set_name(&script.path)
            .unwrap()
            .set_environment(env_table.clone())
            .unwrap()
            .call(())
            .unwrap();
        let registry_key = lua.create_registry_value(loaded).unwrap();
        script_context.queued.push((entity, registry_key));
    }

    drop(fs_guard);
}

pub fn update_scripts(
    context: SystemContext,
    (lua, script_context, fs): (&Lua, &mut ScriptContext, &mut Filesystem),
    (with_registry_key,): (QueryMarker<(&Script, &LuaRegistryKey)>,),
) {
    let fs_guard = script_context.stretched_fs.loan(fs);
}
