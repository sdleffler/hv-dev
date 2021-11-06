use std::io::Read;

use hv::{
    ecs::{Entity, QueryMarker, SystemContext, Without},
    fs::Filesystem,
    prelude::*,
    sync::elastic::Elastic,
};
use tracing::{error, trace_span};

use crate::types::Dt;

#[derive(Debug)]
pub struct Script {
    pub path: String,
}

pub struct ScriptContext {
    // Environment table for scripts loaded in this context
    env: LuaRegistryKey,
    stretched_fs: Elastic<*mut Filesystem>,
    // Queued entities to have their newly loaded scripts attached (or an error if loading the
    // script failed for some reason).
    queued: Vec<(Entity, Result<LuaRegistryKey, Error>)>,
}

pub fn script_upkeep_system(
    context: SystemContext,
    (lua, script_context, fs): (&Lua, &mut ScriptContext, &mut Filesystem),
    (without_registry_key,): (QueryMarker<Without<LuaRegistryKey, &Script>>,),
) {
    let _span = trace_span!("script_upkeep_system").entered();

    let fs_guard = script_context.stretched_fs.loan(fs);
    let mut buf = String::new();
    let env_table: LuaTable = lua.registry_value(&script_context.env).unwrap();

    for (entity, script) in context.query(without_registry_key).iter() {
        let res = (|| -> Result<LuaRegistryKey> {
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
            Ok(registry_key)
        })();

        // match res {

        // }
    }

    drop(fs_guard);
}

pub fn script_update_system(
    context: SystemContext,
    (lua, script_context, fs, &Dt(dt)): (&Lua, &mut ScriptContext, &mut Filesystem, &Dt),
    (with_registry_key,): (QueryMarker<(&Script, &LuaRegistryKey)>,),
) {
    let _span = trace_span!("script_update_system").entered();

    let fs_guard = script_context.stretched_fs.loan(fs);

    for (entity, (script, key)) in context.query(with_registry_key).iter() {
        let res = (|| -> Result<()> {
            let table = lua.registry_value::<LuaTable>(key).unwrap();
            if table.contains_key("update")? {
                let _: () = table.call_method("update", (dt,))?;
            }
            Ok(())
        })();

        if let Err(err) = res {
            error!(
                entity = ?entity,
                script = ?script.path,
                error = ?err,
                "error calling entity script update: {:#}",
                err
            );
        }
    }

    drop(fs_guard);
}
