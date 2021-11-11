#![cfg(feature = "macros")]

use hv::lua::{chunk, from_table::FromTable, Lua, Result};

#[test]
fn test_chunk_macro() -> Result<()> {
    let lua = Lua::new();

    let name = "Rustacean";
    let table = FromTable(vec![1]);

    let data = lua.create_table()?;
    data.raw_set("num", 1)?;

    lua.globals().set("g", 123)?;

    lua.load(chunk! {
        assert($name == "Rustacean")
        assert($table[1] == 1)
        assert($data.num == 1)
        assert(g == 123)
        s = 321
    })
    .exec()?;

    assert_eq!(lua.globals().get::<_, i32>("s")?, 321);

    Ok(())
}
