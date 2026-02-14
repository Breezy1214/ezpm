local fs = require("@lune/fs")
local process = require("@lune/process")

local utils = require("./utils")

local PACKAGE_PATH: string? = nil
local TOOL_AVAILABILITY_CACHE: { [string]: boolean } = {}

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

local function setupWallyPackages()
	print("Setting up Wally packages...")
	print("Clearing current systems...")
	if fs.isFile("sourcemap.json") then
		fs.removeFile("sourcemap.json")
	end
	if fs.isFile("wally.lock") then
		fs.removeFile("wally.lock")
	end
	if fs.isDir("Packages") then
		fs.removeDir("Packages")
	end
	if fs.isDir("ServerPackages") then
		fs.removeDir("ServerPackages")
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

	print("Setting up package types...")
	local typesResult = executeViaToolchainManager("wally-package-types", "--sourcemap", "sourcemap.json", "Packages")
	if not typesResult.ok then
		warn("Package types generation failed:", typesResult.stderr)
	else
		print("Package types generated successfully")
	end

	print("Setting up ServerPackages types...")
	local serverTypesResult =
		executeViaToolchainManager("wally-package-types", "--sourcemap", "sourcemap.json", "ServerPackages")
	if not serverTypesResult.ok then
		warn("ServerPackages types generation failed:", serverTypesResult.stderr)
	else
		print("ServerPackages types generated successfully")
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
		print("Install them with: rokit add Kampfkarren/selene@0.27.1")
		print("                    rokit add JohnnyMorganz/StyLua@2.0.2")
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
		print("Skipping Selene (not installed — run `rokit add Kampfkarren/selene@0.27.1`)")
	end

	if hasStylua then
		print("Checking StyLua formatting...")
		local styluaResult = executeViaToolchainManager("stylua", "--check", srcPath)
		if not styluaResult.ok then
			warn("StyLua formatting issues found:")
			print(styluaResult.stdout)
			print("Run 'stylua src/' to fix formatting issues")
			return false
		else
			print("✓ StyLua formatting is correct")
		end
	else
		print("Skipping StyLua (not installed — run `rokit add JohnnyMorganz/StyLua@2.0.2`)")
	end

	print("✓ All code quality checks passed!")
	return true
end

local function formatCode(srcPath: string)
	if not isToolAvailable("stylua") then
		warn("StyLua is not installed.")
		print("Install it with: rokit add JohnnyMorganz/StyLua@2.0.2")
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

local function installTools()
	print("Installing development tools...")

	print("Installing Rokit tools...")
	local rokitResult = utils.executeString("rokit install")
	if not rokitResult.ok then
		warn("Rokit install failed:", rokitResult.stderr)
		return false
	end
	print("✓ Rokit tools installed")

	print("Installing Wally packages...")
	setupWallyPackages()

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
}
