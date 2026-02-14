--[[
	ezpm - EZ Project Manager CLI
	Entry point: arg parsing, command dispatch, interactive menu, init command

	Usage:
		lune run ezpm                  # Interactive menu
		lune run ezpm serve            # Watch + DarkLua + Rojo serve
		lune run ezpm fix-requires     # One-shot require path fix
		lune run ezpm install          # rokit install + Wally packages
		lune run ezpm lint             # Selene + StyLua --check
		lune run ezpm format           # StyLua format
		lune run ezpm init             # Scaffold project + generate ezpm.toml
		lune run ezpm docs             # Moonwave dev
		lune run ezpm help             # Print usage
]]

local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local stdio = require("@lune/stdio")

local config = require("./config")
local utils = require("./utils")
local requireFixer = require("./require-fixer")
local toolchain = require("./toolchain")
local serve = require("./serve")

-- Load configuration
local CONFIG = config.loadConfig()

------------------------// Help //--------------------------

local HELP_TEXT = [[
ezpm - EZ Project Manager

Usage:
  lune run ezpm [command]

Commands:
  (none)          Interactive menu
  serve           Watch files + DarkLua + Rojo serve
  fix-requires    One-shot require path fix
  install         Install tools (rokit install + Wally packages)
  lint            Run Selene linter + StyLua --check (if installed)
  format          Format code with StyLua (if installed)
  init            Scaffold project + generate ezpm.toml
  docs            Open documentation via Moonwave
  help            Print this help message

Flags:
  -h, --help      Print this help message
]]

local function printHelp()
	print(HELP_TEXT)
end

------------------------// Init Command //--------------------------

local function runInit()
	print("")
	print("=== ezpm init ===")
	print("Scaffolding project configuration...")
	print("")

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
				print("  Detected project name:", projectName)
			end
		end
	end

	if hasDarklua then
		print("  Detected .darklua.json")
	end
	if hasRokit then
		print("  Detected rokit.toml")
	end
	if hasWally then
		print("  Detected wally.toml")
	end

	-- Prompt to scaffold rokit if missing
	if not hasRokit then
		local scaffoldRokit = stdio.prompt("confirm", "No rokit.toml found. Create one with default tools?")
		if scaffoldRokit then
			local rokitToml = [[
# This file lists tools managed by Rokit, a toolchain manager for Roblox projects.
# For more information, see https://github.com/rojo-rbx/rokit

[tools]
lune = "lune-org/lune@0.10.2"
rojo = "rojo-rbx/rojo@7.4.4"
darklua = "seaofvoices/darklua@0.14.1"
wally = "UpliftGames/wally@0.3.2"
wally-package-types = "JohnnyMorganz/wally-package-types@1.3.2"

# Optional — install with `rokit add <tool>`
# selene = "Kampfkarren/selene@0.27.1"
# stylua = "JohnnyMorganz/StyLua@2.0.2"
]]
			fs.writeFile("rokit.toml", rokitToml)
			print("  Created rokit.toml")
		end
	end

	-- Create missing required directories
	local requiredDirs = { "src", "src/client", "src/server", "src/shared" }
	for _, dir in ipairs(requiredDirs) do
		if not fs.isDir(dir) then
			fs.writeDir(dir)
			print("  Created directory:", dir)
		end
	end

	-- Generate ezpm.toml
	local initConfig = config.getDefaultConfig()
	initConfig.PROJECT_NAME = projectName

	local tomlString = config.generateTomlString(initConfig)
	fs.writeFile("ezpm.toml", tomlString)
	print("")
	print("✓ Generated ezpm.toml")
	print("")
	print("You can now run 'lune run ezpm' to start the interactive menu,")
	print("or 'lune run ezpm serve' to start developing.")
end

------------------------// Interactive Menu //--------------------------

local function runInteractiveMenu()
	print("")
	print("=== EZ Project Manager ===")
	print("Modular CLI with comprehensive development tools")
	print("")

	print("Project Manager ready! Choose an option below:")

	local Options = {
		[1] = {
			Name = "Setup Wally Packages",
			Description = "Installs the latest Wally Packages, generates their type defs, and sets up the source map.",
			Execute = toolchain.setupWallyPackages,
		},
		[2] = {
			Name = "Serve Rojo",
			Description = "Processes the Dark Lua files, watches for changes, and serves the Rojo project with live file monitoring.",
			Execute = function()
				serve.start(CONFIG)
			end,
		},
		[3] = {
			Name = "Fix require paths",
			Description = "Automatically fixes require paths in all files in the '" .. CONFIG.SRC_PATH .. "' directory.",
			Execute = function()
				requireFixer.fixRequirePaths(CONFIG.SRC_PATH, CONFIG.DISPLAY_FILE_CHANGES, CONFIG.IGNORE_LIST)
			end,
		},
		[4] = {
			Name = "Run Code Quality Checks",
			Description = "Runs Selene linting and StyLua formatting checks (if installed).",
			Execute = function()
				toolchain.runLinting(CONFIG.SRC_PATH)
			end,
		},
		[5] = {
			Name = "Format Code",
			Description = "Formats all code using StyLua (if installed).",
			Execute = function()
				toolchain.formatCode(CONFIG.SRC_PATH)
			end,
		},
		[6] = {
			Name = "Install Tools",
			Description = "Installs all development tools (Rokit + Wally packages).",
			Execute = toolchain.installTools,
		},
		[7] = {
			Name = "Open Documentation",
			Description = "Opens the local documentation for the project via moonwave.",
			Execute = function()
				toolchain.openDocumentation(CONFIG.DOCS_ENABLED)
			end,
		},
		[0] = {
			Name = "EXIT",
			Description = "Exits the program.",
			Execute = function()
				return true
			end,
		},
	}

	local EXIT = false

	while not EXIT do
		print("-------------")
		print("| Main Menu |")
		print("-------------")
		for i, option in Options do
			if option then
				stdio.write(stdio.style("bold"))
				stdio.write(i .. ") ")
				utils.resetStyle()
				print(option.Name)
				stdio.write(stdio.color("black"))
				print("   - " .. option.Description)
				utils.resetStyle()
			end
		end
		print("-------------")

		local selectedOption = nil
		repeat
			local input = stdio.prompt("text", "ENTER OPTION: ")
			local inputArgs = string.split(input, " ")
			local number = tonumber(inputArgs[1])
			if number and Options[number] then
				selectedOption = Options[number]
			else
				warn("Invalid Choice. Please try again.")
			end
		until selectedOption

		if selectedOption.Execute then
			local shouldExit = selectedOption.Execute()
			if shouldExit then
				EXIT = true
			end
		else
			warn("No execute function found for option.")
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
		toolchain.installTools()
	end,
	lint = function()
		toolchain.runLinting(CONFIG.SRC_PATH)
	end,
	format = function()
		toolchain.formatCode(CONFIG.SRC_PATH)
	end,
	init = runInit,
	docs = function()
		toolchain.openDocumentation(CONFIG.DOCS_ENABLED)
	end,
	help = printHelp,
}

-- Parse args
local subcommand = process.args[1]

-- Check for help flags anywhere in args
for _, arg in ipairs(process.args) do
	if arg == "-h" or arg == "--help" then
		printHelp()
		return
	end
end

if subcommand then
	local handler = COMMANDS[subcommand]
	if handler then
		handler()
	else
		warn(`Unknown command: {subcommand}`)
		print("")
		printHelp()
		process.exit(1)
	end
else
	-- No subcommand: run interactive menu
	runInteractiveMenu()
end
