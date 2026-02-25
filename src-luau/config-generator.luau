local fs = require("@lune/fs")
local serde = require("@lune/serde")

local DARKLUA_OPTIMIZATION_RULES = {
	"compute_expression",
	"remove_unused_if_branch",
	"remove_unused_while",
	"filter_after_early_return",
	"remove_nil_declaration",
	"remove_empty_do",
}

local function generateDarkluaJson(aliases: { [string]: string }): string
	local darkluaConfig = {
		process = {
			{
				rule = "convert_require",
				current = {
					name = "luau",
				},
				target = {
					name = "roblox",
					rojo_sourcemap = "sourcemap.json",
					indexing_style = "find_first_child",
				},
			},
		},
	}

	for _, rule in DARKLUA_OPTIMIZATION_RULES do
		table.insert(darkluaConfig.process, rule)
	end

	return serde.encode("json", darkluaConfig, true)
end

local function getLuneVersion(): string?
	if not fs.isFile("rokit.toml") then
		return nil
	end

	local success, contents = pcall(fs.readFile, "rokit.toml")
	if not success then
		return nil
	end

	local parseSuccess, rokitConfig = pcall(serde.decode, "toml", contents)
	if not parseSuccess then
		return nil
	end

	if rokitConfig.tools and rokitConfig.tools.lune then
		local version = rokitConfig.tools.lune:match("@(.+)$")
		return version
	end

	return nil
end

local function generateLuaurc(aliases: { [string]: string }): string
	local luaurcAliases = {}

	local luneVersion = getLuneVersion()
	if luneVersion then
		luaurcAliases["lune"] = "~/.lune/.typedefs/" .. luneVersion
	end

	for name, path in aliases do
		luaurcAliases[name] = path
	end

	local luaurc = {
		aliases = luaurcAliases,
	}

	return serde.encode("json", luaurc, true)
end

local function regenerateConfigFiles(aliases: { [string]: string })
	local darkluaJson = generateDarkluaJson(aliases)
	fs.writeFile(".darklua.json", darkluaJson)

	local luaurc = generateLuaurc(aliases)
	fs.writeFile(".luaurc", luaurc)
end

local function importAliasesFromDarklua(): { [string]: string }
	local aliases = {}

	-- Try .luaurc first (used by luau require mode)
	if fs.isFile(".luaurc") then
		local success, contents = pcall(fs.readFile, ".luaurc")
		if success then
			local parseSuccess, luaurc = pcall(serde.decode, "json", contents)
			if parseSuccess and luaurc and luaurc.aliases then
				for name, path in luaurc.aliases do
					-- Skip the lune typedef alias — it's not a user alias
					if name ~= "lune" then
						aliases[name] = path
					end
				end
				if next(aliases) ~= nil then
					return aliases
				end
			end
		end
	end

	-- Fall back to .darklua.json sources (legacy path mode)
	if not fs.isFile(".darklua.json") then
		return aliases
	end

	local success, contents = pcall(fs.readFile, ".darklua.json")
	if not success then
		return aliases
	end

	local parseSuccess, darkluaConfig = pcall(serde.decode, "json", contents)
	if not parseSuccess then
		return aliases
	end

	local sources = darkluaConfig
		and darkluaConfig.process
		and darkluaConfig.process[1]
		and darkluaConfig.process[1].current
		and darkluaConfig.process[1].current.sources

	if sources then
		for alias, path in sources do
			local name = alias:match("^@(.+)$") or alias
			aliases[name] = path
		end
	end

	return aliases
end

return {
	generateDarkluaJson = generateDarkluaJson,
	generateLuaurc = generateLuaurc,
	regenerateConfigFiles = regenerateConfigFiles,
	importAliasesFromDarklua = importAliasesFromDarklua,
}
