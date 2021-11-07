use std::io::Read;

use hv::{
    ecs::{QueryMarker, SystemContext, Without},
    fs::Filesystem,
    prelude::*,
    sync::elastic::{Elastic, StretchedMut},
};
use tracing::{error, trace_span};

use crate::{command_buffer::CommandPoolResource, types::Dt};

#[derive(Debug)]
pub struct Script {
    pub path: String,
}

#[derive(Debug)]
pub struct ScriptLoadError {
    pub error: Error,
}

pub struct ScriptContext {
    // Environment table for scripts loaded in this context
    env: LuaRegistryKey,
    stretched_fs: Elastic<StretchedMut<Filesystem>>,
}

static_assertions::assert_impl_all!(ScriptContext: Send, Sync);

pub fn script_upkeep_system(
    context: SystemContext,
    (script_context, fs, command_pool): (&mut ScriptContext, &mut Filesystem, &CommandPoolResource),
    lua: &Lua,
    (unloaded_scripts,): (QueryMarker<Without<ScriptLoadError, Without<LuaRegistryKey, &Script>>>,),
) {
    let _span = trace_span!("script_upkeep_system").entered();

    let _fs_guard = script_context.stretched_fs.loan(fs);
    let mut command_buffer = command_pool.get_buffer();

    let mut buf = String::new();
    let env_table: LuaTable = lua.registry_value(&script_context.env).unwrap();

    for (entity, script) in context.query(unloaded_scripts).iter() {
        let res = (|| -> Result<LuaRegistryKey> {
            let mut mut_fs = script_context.stretched_fs.borrow_mut().unwrap();
            buf.clear();
            mut_fs.open(&script.path)?.read_to_string(&mut buf)?;
            drop(mut_fs);
            let loaded: LuaValue = lua
                .load(&buf)
                .set_name(&script.path)?
                .set_environment(env_table.clone())?
                .call(())?;
            let registry_key = lua.create_registry_value(loaded)?;
            Ok(registry_key)
        })();

        match res {
            Ok(key) => command_buffer.insert(entity, (key,)),
            Err(error) => {
                error!(
                    ?entity,
                    script = ?script.path,
                    ?error,
                    "error instantiating entity script: {:#}",
                    error
                );

                command_buffer.insert(entity, (ScriptLoadError { error },));
            }
        }
    }
}

pub fn script_update_system(
    context: SystemContext,
    (script_context, fs, &Dt(dt)): (&mut ScriptContext, &mut Filesystem, &Dt),
    lua: &Lua,
    (with_registry_key,): (QueryMarker<(&Script, &LuaRegistryKey)>,),
) {
    let _span = trace_span!("script_update_system").entered();

    let _fs_guard = script_context.stretched_fs.loan(fs);

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
}
