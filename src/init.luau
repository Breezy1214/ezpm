--[[
	ezpm - EZ Project Manager CLI
	Interactive menu-first interface with subcommand shortcuts.

	Usage:
		ezpm            # Interactive menu (primary interface)
		ezpm <command>  # Direct subcommand shortcut
		ezpm help       # List available subcommands
]]

local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local stdio = require("@lune/stdio")

local config = require("@self/config")
local configGenerator = require("@self/config-generator")
local utils = require("@self/utils")
local requireFixer = require("@self/require-fixer")
local toolchain = require("@self/toolchain")
local serve = require("@self/serve")
local darklua = require("@self/darklua")

-- Load configuration
local CONFIG = config.loadConfig()

------------------------// Help //--------------------------

local HELP_TEXT = [[
ezpm - EZ Project Manager

Usage:
  ezpm         	Interactive menu (primary interface)

Subcommands:
  serve         Dev server (file watcher + DarkLua + Rojo)
  fix-requires  One-shot require path fix
  install       Install tools (rokit + Wally packages)
  lint          Selene + StyLua --check
  format        StyLua format
  init          Scaffold project + generate ezpm.toml
  alias         Manage path aliases (add/remove/list)
  docs          Moonwave documentation server
  help          Print this help message

Flags:
  -h, --help    Print this help message
]]

local function printHelp()
	print(HELP_TEXT)
end

------------------------// Alias Commands //--------------------------

local function aliasAdd()
	local name = stdio.prompt("text", "Alias name (e.g., Client): ")
	if not name or name == "" then
		print("Aborted.")
		return
	end

	local path = stdio.prompt("text", "Path (e.g., src/client/): ")
	if not path or path == "" then
		print("Aborted.")
		return
	end

	-- Ensure trailing slash
	if not path:match("/$") then
		path = path .. "/"
	end

	CONFIG.ALIASES[name] = path
	config.saveConfig(CONFIG)
	configGenerator.regenerateConfigFiles(CONFIG.ALIASES)

	-- Update derived fields
	CONFIG.IGNORE_LIST = config.deriveIgnoreList(CONFIG.ALIASES, CONFIG.SRC_PATH)
	CONFIG.REQUIRED_DIRS = config.deriveRequiredDirs(CONFIG.ALIASES, CONFIG.SRC_PATH)

	-- Optionally create directory
	local dir = path:gsub("/$", "")
	if not fs.isDir(dir) then
		local createDir = stdio.prompt("confirm", "Create directory '" .. dir .. "'?")
		if createDir then
			fs.writeDir(dir)
			utils.printDim("  Created directory: " .. dir)
		end
	end

	darklua.clearCache()
	utils.printSuccess("Added alias @" .. name .. " -> " .. path)
end

local function aliasRemove()
	local names = {}
	for name in CONFIG.ALIASES do
		table.insert(names, name)
	end
	table.sort(names)

	if #names == 0 then
		print("No aliases configured.")
		return
	end

	-- Build labels for select prompt
	local labels = {}
	for _, name in names do
		table.insert(labels, name .. " -> " .. CONFIG.ALIASES[name])
	end
	table.insert(labels, "Cancel")

	local selected = stdio.prompt("select", "Select alias to remove", labels)
	if selected == nil or selected == #labels then
		return
	end

	local selectedName = names[selected]

	local confirm = stdio.prompt("confirm", "Remove alias @" .. selectedName .. "?")
	if not confirm then
		return
	end

	CONFIG.ALIASES[selectedName] = nil
	config.saveConfig(CONFIG)
	configGenerator.regenerateConfigFiles(CONFIG.ALIASES)

	CONFIG.IGNORE_LIST = config.deriveIgnoreList(CONFIG.ALIASES, CONFIG.SRC_PATH)
	CONFIG.REQUIRED_DIRS = config.deriveRequiredDirs(CONFIG.ALIASES, CONFIG.SRC_PATH)

	darklua.clearCache()
	utils.printSuccess("Removed alias @" .. selectedName)
end

local function aliasList()
	local names = {}
	for name in CONFIG.ALIASES do
		table.insert(names, name)
	end
	table.sort(names)

	if #names == 0 then
		print("No aliases configured.")
		return
	end

	print("")
	for _, name in names do
		stdio.write(stdio.style("bold"))
		stdio.write("  @" .. name)
		utils.resetStyle()
		utils.printDim("    -> " .. CONFIG.ALIASES[name])
	end
	print("")
end

local function aliasMenu()
	local options = { "Add Alias", "Remove Alias", "List Aliases", "Back" }

	while true do
		utils.printHeader("Alias Management")
		local selected = stdio.prompt("select", "Choose an action", options)
		if selected == nil or selected == #options then
			break
		end

		if selected == 1 then
			aliasAdd()
		elseif selected == 2 then
			aliasRemove()
		elseif selected == 3 then
			aliasList()
		end
	end
end

------------------------// Init Command //--------------------------

local function runInit()
	utils.printHeader("Initialize Project", "Scaffolding project configuration")

	-- Check if ezpm.toml already exists
	if fs.isFile("ezpm.toml") then
		local overwrite = stdio.prompt("confirm", "ezpm.toml already exists. Overwrite?")
		if not overwrite then
			print("Aborted.")
			return
		end
	end

	-- Detect existing project files
	local projectName = "my-roblox-game"
	local hasDarklua = fs.isFile(".darklua.json")
	local hasRokit = fs.isFile("rokit.toml")
	local hasWally = fs.isFile("wally.toml")

	if fs.isFile("default.project.json") then
		local success, contents = pcall(fs.readFile, "default.project.json")
		if success then
			local parseSuccess, projectJson = pcall(serde.decode, "json", contents)
			if parseSuccess and projectJson.name then
				projectName = projectJson.name
				utils.printDim("  Detected project name: " .. projectName)
			end
		end
	end

	if hasDarklua then
		utils.printDim("  Detected .darklua.json")
	end

	if hasRokit then
		utils.printDim("  Detected rokit.toml")
	end

	if hasWally then
		utils.printDim("  Detected wally.toml")
	end

	-- Prompt to scaffold rokit if missing
	if not hasRokit then
		local scaffoldRokit = stdio.prompt("confirm", "No rokit.toml found. Create one with default tools?")
		if scaffoldRokit then
			fs.writeFile("rokit.toml", toolchain.generateRokitToml())
			utils.printDim("  Created rokit.toml")
		end
	end

	-- Prompt for source directory
	local srcPath = stdio.prompt("text", "Source directory (default: src): ")
	if srcPath == nil or srcPath == "" then
		srcPath = "src"
	end

	-- Handle aliases
	local initConfig = config.getDefaultConfig()
	initConfig.PROJECT_NAME = projectName
	initConfig.SRC_PATH = srcPath

	local aliases = {}

	if hasDarklua then
		local importAliases = stdio.prompt("confirm", "Import aliases from existing .darklua.json?")
		if importAliases then
			aliases = configGenerator.importAliasesFromDarklua()
			local aliasCount = 0

			for _ in aliases do
				aliasCount += 1
			end

			if aliasCount > 0 then
				utils.printDim("  Imported " .. aliasCount .. " aliases from .darklua.json")
			end
		end
	end

	-- If no aliases yet, offer defaults
	if next(aliases) == nil then
		local useDefaults =
			stdio.prompt("confirm", "Use default aliases (Client, Server, Shared, Packages, ServerPackages)?")
		if useDefaults then
			aliases = {
				Client = initConfig.SRC_PATH .. "/client/",
				Server = initConfig.SRC_PATH .. "/server/",
				Shared = initConfig.SRC_PATH .. "/shared/",
				Packages = "Packages/",
				ServerPackages = "ServerPackages/",
			}
		end
	end

	initConfig.ALIASES = aliases

	-- Create directories derived from aliases
	local dirsToCreate = config.deriveRequiredDirs(aliases, initConfig.SRC_PATH)
	for _, dir in dirsToCreate do
		if not fs.isDir(dir) then
			fs.writeDir(dir)
			utils.printDim("  Created directory: " .. dir)
		end
	end

	-- Save ezpm.toml
	config.saveConfig(initConfig)
	print("")
	utils.printSuccess("Generated ezpm.toml")

	-- Regenerate .darklua.json and .luaurc from aliases
	if next(aliases) ~= nil then
		configGenerator.regenerateConfigFiles(aliases)
		utils.printSuccess("Generated .darklua.json")
		utils.printSuccess("Generated .luaurc")
	end

	-- Scaffold default.project.json if missing
	if not fs.isFile("default.project.json") then
		local projectTree: { [string]: any } = {
			["$className"] = "DataModel",
		}

		-- Map aliases to Rojo tree locations using common conventions
		local serviceMap: { [string]: { service: string, container: string? } } = {
			Client = { service = "StarterPlayer", container = "StarterPlayerScripts" },
			Server = { service = "ServerScriptService" },
			Shared = { service = "ReplicatedStorage" },
		}

		local srcPath = initConfig.SRC_PATH
		local escapedSrc = srcPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1")

		for aliasName, aliasPath in aliases do
			if aliasPath:match("^" .. escapedSrc .. "/") then
				local mapping = serviceMap[aliasName]
				if mapping then
					if mapping.container then
						if not projectTree[mapping.service] then
							projectTree[mapping.service] = {}
						end
						projectTree[mapping.service][mapping.container] = {
							[aliasName] = { ["$path"] = aliasPath:gsub("/$", "") },
						}
					else
						if not projectTree[mapping.service] then
							projectTree[mapping.service] = {}
						end
						projectTree[mapping.service][aliasName] = { ["$path"] = aliasPath:gsub("/$", "") }
					end
				else
					-- Unknown alias under src — default to ReplicatedStorage
					if not projectTree["ReplicatedStorage"] then
						projectTree["ReplicatedStorage"] = {}
					end
					projectTree["ReplicatedStorage"][aliasName] = { ["$path"] = aliasPath:gsub("/$", "") }
				end
			end
		end

		local projectJson = {
			name = initConfig.PROJECT_NAME or "my-roblox-game",
			tree = projectTree,
		}
		fs.writeFile("default.project.json", serde.encode("json", projectJson))
		utils.printSuccess("Generated default.project.json")
	end

	-- Reload config
	CONFIG = config.loadConfig()

	print("")
	print("Select Serve from the menu to start developing,")
	print("or run 'ezpm serve' directly.")
end

------------------------// Interactive Menu //--------------------------

local function runInteractiveMenu()
	local options = {
		{
			label = "Serve",
			execute = function()
				serve.start(CONFIG)
			end,
		},
		{
			label = "Setup Wally Packages",
			execute = function()
				toolchain.setupWallyPackages(CONFIG)
			end,
		},
		{
			label = "Fix Require Paths",
			execute = function()
				requireFixer.fixRequirePaths(CONFIG.SRC_PATH, CONFIG.DISPLAY_FILE_CHANGES, CONFIG.IGNORE_LIST)
			end,
		},
		{
			label = "Run Code Quality Checks",
			execute = function()
				toolchain.runLinting(CONFIG.SRC_PATH)
			end,
		},
		{
			label = "Format Code",
			execute = function()
				toolchain.formatCode(CONFIG.SRC_PATH)
			end,
		},
		{
			label = "Install Tools",
			execute = function()
				toolchain.installTools(CONFIG)
			end,
		},
		{ label = "Manage Aliases", execute = aliasMenu },
		{ label = "Initialize Project", execute = runInit },
		{
			label = "Open Documentation",
			execute = function()
				toolchain.openDocumentation(CONFIG.DOCS_ENABLED)
			end,
		},
		{ label = "Exit" },
	}

	local labels = {}
	for _, option in options do
		table.insert(labels, option.label)
	end

	while true do
		utils.printHeader("ezpm", "Roblox Project Manager")
		local selected = stdio.prompt("select", "What would you like to do?", labels)
		if selected == nil or selected == #options then
			break
		end

		local option = options[selected]
		if option.execute then
			option.execute()
		end
	end

	print("Goodbye!")
end

------------------------// CLI Command Dispatch //--------------------------

local COMMANDS = {
	serve = function()
		serve.start(CONFIG)
	end,
	["fix-requires"] = function()
		requireFixer.fixRequirePaths(CONFIG.SRC_PATH, CONFIG.DISPLAY_FILE_CHANGES, CONFIG.IGNORE_LIST)
	end,
	install = function()
		toolchain.installTools(CONFIG)
	end,
	lint = function()
		toolchain.runLinting(CONFIG.SRC_PATH)
	end,
	format = function()
		toolchain.formatCode(CONFIG.SRC_PATH)
	end,
	init = runInit,
	alias = function()
		local subAction = process.args[2]
		if subAction == "add" then
			aliasAdd()
		elseif subAction == "remove" then
			aliasRemove()
		elseif subAction == "list" then
			aliasList()
		else
			aliasMenu()
		end
	end,
	docs = function()
		toolchain.openDocumentation(CONFIG.DOCS_ENABLED)
	end,
	help = printHelp,
}

local subcommand = process.args[1]

if subcommand == "-h" or subcommand == "--help" then
	printHelp()
elseif subcommand and COMMANDS[subcommand] then
	COMMANDS[subcommand]()
else
	if subcommand then
		warn(`Unknown command: {subcommand}`)
		print("")
		printHelp()
		process.exit(1)
	end
	runInteractiveMenu()
end
