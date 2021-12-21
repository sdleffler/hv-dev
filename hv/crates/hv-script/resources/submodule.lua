local name = ...

-- If we're passed a module name, then overwrite `require` such that it first checks whether we're
-- trying to load a submodule. That way, we can portably write code within this module w/o worrying
-- about import paths.
if name then
    local env = getfenv(2)
    return function(submodule)
        -- Submodules of this one inherit its environment.
        local full = name .. "." .. submodule
        local loaded = package.loaded[full]
        if loaded then return loaded end

        local ok, result = pcall(loadfile, full)
        if ok and type(result) == "function" then
            package.preload[full] = setfenv(result, env)
        else
            error(result)
        end

        return _G.require(full)
    end
else
    return _G.require
end
