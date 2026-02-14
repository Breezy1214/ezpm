
local fs = require("@lune/fs")
local serde = require("@lune/serde")

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
}

local function getDefaultConfig(): EzpmConfig
	return {
		SRC_PATH = "src",
		DARKLUA_BUILD_PATH = "darklua_build",
		DOCS_ENABLED = false,
		DISPLAY_FILE_CHANGES = true,
		HELP_ARGS = { "-h", "--help", "help" },
		IGNORE_LIST = {
			"@Packages/",
			"@ServerPackages/",
			"@DevPackages/",
		},
		REQUIRED_DIRS = { "src", "src/client", "src/server", "src/shared" },
		REQUIRED_FILES = { "default.project.json", "wally.toml", "rokit.toml" },
	}
end

local function loadConfig(): EzpmConfig
	local config = getDefaultConfig()

	if not fs.isFile("ezpm.toml") then
		return config
	end

	local success, contents = pcall(fs.readFile, "ezpm.toml")
	if not success then
		warn("Failed to read ezpm.toml, using defaults")
		return config
	end

	local parseSuccess, tomlConfig = pcall(serde.decode, "toml", contents)
	if not parseSuccess then
		warn("Failed to parse ezpm.toml, using defaults")
		return config
	end

	-- Merge toml values into config
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
	end

	return config
end

local function generateTomlString(config: EzpmConfig): string
	local tomlData = {
		project = {
			name = config.PROJECT_NAME or "my-roblox-game",
		},
		paths = {
			src = config.SRC_PATH,
			darklua_build = config.DARKLUA_BUILD_PATH,
		},
		display = {
			file_changes = config.DISPLAY_FILE_CHANGES,
			docs_enabled = config.DOCS_ENABLED,
		},
	}

	return serde.encode("toml", tomlData)
end

return {
	loadConfig = loadConfig,
	getDefaultConfig = getDefaultConfig,
	generateTomlString = generateTomlString,
}
