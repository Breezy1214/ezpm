
local fs = require("@lune/fs")
local serde = require("@lune/serde")

local configGenerator = require("./config-generator")

export type EzpmConfig = {
	SRC_PATH: string,
	DARKLUA_BUILD_PATH: string,
	DOCS_ENABLED: boolean,
	DISPLAY_FILE_CHANGES: boolean,
	HELP_ARGS: { string },
	IGNORE_LIST: { string },
	REQUIRED_DIRS: { string },
	REQUIRED_FILES: { string },
	PROJECT_NAME: string?,
	VERSION: string?,
	ALIASES: { [string]: string },
}

local function deriveIgnoreList(aliases: { [string]: string }, srcPath: string): { string }
	local ignoreList = {}
	local escapedSrc = srcPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1")
	for name, path in aliases do
		if not path:match("^" .. escapedSrc .. "/") then
			table.insert(ignoreList, "@" .. name .. "/")
		end
	end
	return ignoreList
end

local function deriveRequiredDirs(aliases: { [string]: string }, srcPath: string): { string }
	local dirsSet = {}
	dirsSet[srcPath] = true

	local escapedSrc = srcPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1")
	for _, path in aliases do
		if path:match("^" .. escapedSrc .. "/") then
			local dir = path:gsub("/$", "")
			dirsSet[dir] = true
			local parts = string.split(dir, "/")
			local accumulated = ""
			for i, part in parts do
				if i == 1 then
					accumulated = part
				else
					accumulated = accumulated .. "/" .. part
				end
				dirsSet[accumulated] = true
			end
		end
	end

	local dirs = {}
	for dir in dirsSet do
		table.insert(dirs, dir)
	end
	table.sort(dirs)
	return dirs
end

local function getDefaultConfig(): EzpmConfig
	return {
		SRC_PATH = "src",
		DARKLUA_BUILD_PATH = "darklua_build",
		DOCS_ENABLED = false,
		DISPLAY_FILE_CHANGES = true,
		HELP_ARGS = { "-h", "--help", "help" },
		IGNORE_LIST = {},
		REQUIRED_DIRS = {},
		REQUIRED_FILES = { "rokit.toml" },
		ALIASES = {},
	}
end

local function loadConfig(): EzpmConfig
	local config = getDefaultConfig()

	if fs.isFile("ezpm.toml") then
		local success, contents = pcall(fs.readFile, "ezpm.toml")
		if success then
			local parseSuccess, tomlConfig = pcall(serde.decode, "toml", contents)
			if parseSuccess then
				if tomlConfig.paths then
					if tomlConfig.paths.src then
						config.SRC_PATH = tomlConfig.paths.src
					end
					if tomlConfig.paths.darklua_build then
						config.DARKLUA_BUILD_PATH = tomlConfig.paths.darklua_build
					end
				end

				if tomlConfig.display then
					if tomlConfig.display.file_changes ~= nil then
						config.DISPLAY_FILE_CHANGES = tomlConfig.display.file_changes
					end

					if tomlConfig.display.docs_enabled ~= nil then
						config.DOCS_ENABLED = tomlConfig.display.docs_enabled
					end
				end

				if tomlConfig.project then
					if tomlConfig.project.name then
						config.PROJECT_NAME = tomlConfig.project.name
					end
					
					if tomlConfig.project.version then
						config.VERSION = tomlConfig.project.version
					end
				end

				if tomlConfig.aliases then
					config.ALIASES = tomlConfig.aliases
				end
			else
				warn("Failed to parse ezpm.toml, using defaults")
			end
		else
			warn("Failed to read ezpm.toml, using defaults")
		end
	end

	-- Migration: if no aliases in config but .darklua.json exists, import from it
	if next(config.ALIASES) == nil and fs.isFile(".darklua.json") then
		config.ALIASES = configGenerator.importAliasesFromDarklua()
	end

	-- Derive IGNORE_LIST and REQUIRED_DIRS from aliases
	config.IGNORE_LIST = deriveIgnoreList(config.ALIASES, config.SRC_PATH)
	config.REQUIRED_DIRS = deriveRequiredDirs(config.ALIASES, config.SRC_PATH)

	return config
end

local function generateTomlString(config: EzpmConfig): string
	local tomlData = {
		project = {
			name = config.PROJECT_NAME or "my-roblox-game",
			version = config.VERSION,
		},

		paths = {
			src = config.SRC_PATH,
			darklua_build = config.DARKLUA_BUILD_PATH,
		},

		display = {
			file_changes = config.DISPLAY_FILE_CHANGES,
			docs_enabled = config.DOCS_ENABLED,
		},
		
		aliases = config.ALIASES,
	}

	return serde.encode("toml", tomlData)
end

local function saveConfig(cfg: EzpmConfig)
	local tomlString = generateTomlString(cfg)
	fs.writeFile("ezpm.toml", tomlString)
end

return {
	loadConfig = loadConfig,
	getDefaultConfig = getDefaultConfig,
	generateTomlString = generateTomlString,
	saveConfig = saveConfig,
	deriveIgnoreList = deriveIgnoreList,
	deriveRequiredDirs = deriveRequiredDirs,
}
