-- This should be already loaded in the package table.
local class = require("hv.lua.class")

local State = class("State")

function State:init() end

function State:message(agent, msg, ...)
    local handler = self[msg]
    if handler then handler(self, agent, ...) end
end

local Agent = class("Agent")

Agent.states = {}

function Agent:init() self.stack = {} end

function Agent:message(msg, ...)
    local stack = self.stack
    local top = stack[#stack]
    if top then top:message(self, msg, ...) end
end

function Agent:push(state, ...)
    local ty = assert(self.states[state], "no such state!")
    local stack = self.stack
    local new_state = ty:create()
    stack[#stack + 1] = new_state
    new_state:init(self, ...)
end

function Agent:switch(state, ...)
    local ty = assert(self.states[state], "no such state!")
    local stack = self.stack
    local new_state = ty:create()
    stack[#stack] = new_state
    new_state:init(self, ...)
end

function Agent:pop()
    local stack = self.stack
    stack[#stack] = nil
end

function Agent:bind(...)
    local to_bind
    if select("#", ...) == 1 and type(select(1, ...)) == "table" then
        to_bind = select(1, ...)
    else
        to_bind = {...}
    end

    for _, method in ipairs(to_bind) do
        local message = self.message
        self[method] = function(this, ...) message(this, method, ...) end
    end
end

function Agent:add_states(states)
    for _, state in ipairs(states) do self.states[state.name] = state end
end

return {Agent = Agent, State = State}
