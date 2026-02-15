local fs = require("@lune/fs")
local process = require("@lune/process")

local utils = require("./utils")
local configModule = require("./config")

type EzpmConfig = configModule.EzpmConfig

local PACKAGE_PATH: string? = nil
local TOOL_AVAILABILITY_CACHE: { [string]: boolean } = {}

type ToolDef = { spec: string, required: boolean }

local DEFAULT_TOOLS: { [string]: ToolDef } = {
	lune = { spec = "lune-org/lune@0.10.4", required = true },
	rojo = { spec = "rojo-rbx/rojo@7.6.1", required = true },
	darklua = { spec = "seaofvoices/darklua@0.17.3", required = true },
	wally = { spec = "UpliftGames/wally@0.3.2", required = true },
	["wally-package-types"] = { spec = "JohnnyMorganz/wally-package-types@1.6.2", required = true },
	selene = { spec = "Kampfkarren/selene@0.30.0", required = false },
	stylua = { spec = "JohnnyMorganz/StyLua@2.3.1", required = false },
}

local function parseRokitTools(): { [string]: string }
	if not fs.isFile("rokit.toml") then
		return {}
	end
	local contents = fs.readFile("rokit.toml")
	local tools = {}
	for name, spec in contents:gmatch('(%S+)%s*=%s*"([^"]+)"') do
		tools[name] = spec
	end
	return tools
end

local function getToolSpec(toolName: string): string
	local rokitTools = parseRokitTools()
	if rokitTools[toolName] then
		return rokitTools[toolName]
	end
	local def = DEFAULT_TOOLS[toolName]
	return if def then def.spec else toolName
end

local function getPackagePath(): string
	if PACKAGE_PATH then
		return PACKAGE_PATH
	end

	local isUsingRokit = fs.isFile("rokit.toml")
	local isUsingAftman = fs.isFile("aftman.toml")

	if isUsingRokit then
		local result = utils.executeString("which rokit")
		local rokitPath = result.stdout
		rokitPath = rokitPath:gsub("/c/", "C:/")
		rokitPath = rokitPath:gsub("/rokit", "/"):gsub("\\rokit", "\\"):gsub(".exe", ""):gsub("\n", "")
		PACKAGE_PATH = rokitPath
	elseif isUsingAftman then
		warn("Aftman is not supported yet.")
		error("Aftman not supported")
	else
		error("No package manager found.")
	end

	return PACKAGE_PATH :: string
end

local function isToolAvailable(toolName: string): boolean
	if TOOL_AVAILABILITY_CACHE[toolName] ~= nil then
		return TOOL_AVAILABILITY_CACHE[toolName]
	end

	local packagePath = getPackagePath()
	local toolPath = packagePath .. toolName
	local available = fs.isFile(toolPath) or fs.isFile(toolPath .. ".exe")
	TOOL_AVAILABILITY_CACHE[toolName] = available
	return available
end

local function executeViaToolchainManager(...: string): utils.ExecResult
	local packagePath = getPackagePath()
	local commands = { ... }
	local rokitTool = table.remove(commands, 1)
	return process.exec(packagePath .. rokitTool, commands, {
		stdio = {
			stdout = "inherit",
			stderr = "inherit",
		},
	})
end

local function generateSourceMap(): utils.ExecResult
	local results = utils.executeWithOutput("rojo", "sourcemap", ".", "-o", "sourcemap.json")
	if not results.ok then
		warn("Failed to generate source map with exit code", results.code)
		if results.stderr ~= "" then
			warn("Rojo Error:", results.stderr)
		end
	elseif results.stderr ~= "" then
		warn("Rojo Warning:", results.stderr)
	end
	return results
end

local function generateSeleneRobloxStd()
	if not isToolAvailable("selene") then
		return
	end

	local result = executeViaToolchainManager("selene", "generate-roblox-std")
	if result.ok then
		utils.modifyFileContents("roblox.yml", function(contents)
			local s, _ = string.gsub(
				contents,
				"  require:.-number",
				[[
  require:
    args:
    - type: number
  require:
    args:
    - type: string]]
			)
			return s
		end)
	else
		warn("Failed to generate selene roblox std:", result.stderr)
	end
end

local function getPackageDirs(cfg: EzpmConfig): { string }
	local dirsSet = {}
	local escapedSrc = cfg.SRC_PATH:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1")
	for _, path in cfg.ALIASES do
		local dir = path:gsub("/$", "")
		if not dir:match("^" .. escapedSrc .. "/") and dir ~= cfg.SRC_PATH then
			local topDir = dir:match("^([^/]+)")
			if topDir then
				dirsSet[topDir] = true
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

local function setupWallyPackages(cfg: EzpmConfig?)
	if not fs.isFile("wally.toml") then
		print("No wally.toml found, skipping Wally setup.")
		return
	end

	print("Setting up Wally packages...")
	print("Clearing current systems...")
	if fs.isFile("sourcemap.json") then
		fs.removeFile("sourcemap.json")
	end
	if fs.isFile("wally.lock") then
		fs.removeFile("wally.lock")
	end

	local packageDirs
	if cfg then
		packageDirs = getPackageDirs(cfg)
	else
		packageDirs = { "Packages", "ServerPackages" }
	end

	for _, dir in packageDirs do
		if fs.isDir(dir) then
			fs.removeDir(dir)
		end
	end

	print("Installing Wally packages...")
	local results = executeViaToolchainManager("wally", "install")
	if not results.ok then
		warn("Wally install failed:", results.stderr)
		return
	end
	print("Wally packages installed successfully")

	print("Generating source map...")
	generateSourceMap()

	for _, dir in packageDirs do
		if fs.isDir(dir) then
			print("Setting up types for " .. dir .. "...")
			local typesResult = executeViaToolchainManager("wally-package-types", "--sourcemap", "sourcemap.json", dir)
			if not typesResult.ok then
				warn(dir .. " types generation failed:", typesResult.stderr)
			else
				print(dir .. " types generated successfully")
			end
		end
	end

	generateSourceMap()
	print("Setup complete!")
end

local function runLinting(srcPath: string)
	print("Running code quality checks...")

	local hasSelene = isToolAvailable("selene")
	local hasStylua = isToolAvailable("stylua")

	if not hasSelene and not hasStylua then
		warn("No linting tools installed.")
		print("Install them with: rokit add " .. getToolSpec("selene"))
		print("                    rokit add " .. getToolSpec("stylua"))
		return false
	end

	if hasSelene then
		print("Running Selene linter...")
		local seleneResult = executeViaToolchainManager("selene", srcPath)
		if not seleneResult.ok then
			warn("Selene linting found issues:")
			print(seleneResult.stdout)
			return false
		else
			print("✓ Selene linting passed")
		end
	else
		print("Skipping Selene (not installed — run `rokit add " .. getToolSpec("selene") .. "`)")
	end

	if hasStylua then
		print("Checking StyLua formatting...")
		local styluaResult = executeViaToolchainManager("stylua", "--check", srcPath)
		if not styluaResult.ok then
			warn("StyLua formatting issues found:")
			print(styluaResult.stdout)
			print("Run 'stylua " .. srcPath .. "/' to fix formatting issues")
			return false
		else
			print("✓ StyLua formatting is correct")
		end
	else
		print("Skipping StyLua (not installed — run `rokit add " .. getToolSpec("stylua") .. "`)")
	end

	print("✓ All code quality checks passed!")
	return true
end

local function formatCode(srcPath: string)
	if not isToolAvailable("stylua") then
		warn("StyLua is not installed.")
		print("Install it with: rokit add " .. getToolSpec("stylua"))
		return false
	end

	print("Formatting code with StyLua...")
	local result = executeViaToolchainManager("stylua", srcPath)
	if not result.ok then
		warn("StyLua formatting failed:", result.stderr)
		return false
	end
	print("✓ Code formatted successfully!")
	return true
end

local function ensureRequiredTools()
	if not fs.isFile("rokit.toml") then
		return
	end

	local tomlContents = fs.readFile("rokit.toml")
	local addedAny = false

	for toolName, def in DEFAULT_TOOLS do
		if not def.required then
			continue
		end
		local pattern = toolName .. "%s*="
		if not tomlContents:match(pattern) then
			print(`Adding {toolName} to rokit.toml...`)
			local result = utils.executeString(`rokit add {def.spec}`)
			if result.ok then
				print(`✓ Added {toolName}`)
				addedAny = true
			else
				warn(`Failed to add {toolName}:`, result.stderr)
			end
		end
	end

	if addedAny then
		TOOL_AVAILABILITY_CACHE = {}
	end
end

local function installTools(cfg: EzpmConfig?)
	print("Installing development tools...")

	ensureRequiredTools()

	print("Installing Rokit tools...")
	local rokitResult = utils.executeString("rokit install")
	if not rokitResult.ok then
		warn("Rokit install failed:", rokitResult.stderr)
		return false
	end
	print("✓ Rokit tools installed")

	if fs.isFile("wally.toml") then
		print("Installing Wally packages...")
		setupWallyPackages(cfg)
	end

	print("✓ All tools installed successfully!")
	return true
end

local function openDocumentation(docsEnabled: boolean)
	if not docsEnabled then
		warn("Documentation is not setup for this project.")
		return
	end
	print("Opening Documentation... This may take a moment.")
	utils.executeString("moonwave.cmd dev")
end

local function generateRokitToml(): string
	local requiredLines = {}
	local optionalLines = {}

	for toolName, def in DEFAULT_TOOLS do
		local line = toolName .. ' = "' .. def.spec .. '"'
		if def.required then
			table.insert(requiredLines, line)
		else
			table.insert(optionalLines, line)
		end
	end

	table.sort(requiredLines)
	table.sort(optionalLines)

	local lines = {
		"# This file lists tools managed by Rokit, a toolchain manager for Roblox projects.",
		"# For more information, see https://github.com/rojo-rbx/rokit",
		"",
		"[tools]",
	}

	for _, line in requiredLines do
		table.insert(lines, line)
	end
	
	for _, line in optionalLines do
		table.insert(lines, line)
	end

	return table.concat(lines, "\n") .. "\n"
end

return {
	getPackagePath = getPackagePath,
	isToolAvailable = isToolAvailable,
	executeViaToolchainManager = executeViaToolchainManager,
	generateSourceMap = generateSourceMap,
	generateSeleneRobloxStd = generateSeleneRobloxStd,
	setupWallyPackages = setupWallyPackages,
	runLinting = runLinting,
	formatCode = formatCode,
	installTools = installTools,
	openDocumentation = openDocumentation,
	generateRokitToml = generateRokitToml,
}
