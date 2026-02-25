local fs = require("@lune/fs")
local process = require("@lune/process")
local stdio = require("@lune/stdio")
local task = require("@lune/task")

local watcher = require("./watcher")
local darklua = require("./darklua")
local requireFixer = require("./require-fixer")
local toolchain = require("./toolchain")
local utils = require("./utils")
local configModule = require("./config")

type EzpmConfig = configModule.EzpmConfig

local function copyMetaFile(fullPath: string, srcPath: string, buildPath: string)
	local destPath = string.gsub(fullPath, srcPath, buildPath, 1)
	local content = fs.readFile(fullPath)
	fs.writeFile(destPath, content)
end

local function copyAllMetaFiles(srcPath: string, buildPath: string)
	local function scanAndCopy(dir: string)
		local items = fs.readDir(dir)
		for _, item in ipairs(items) do
			local itemPath = dir .. "/" .. item
			if fs.isDir(itemPath) then
				scanAndCopy(itemPath)
			elseif item == "init.meta.json" then
				local ok, err = pcall(copyMetaFile, itemPath, srcPath, buildPath)
				if not ok then
					warn("Failed to copy meta file:", itemPath, err)
				end
			end
		end
	end
	scanAndCopy(srcPath)
end

local function removeMetaFile(fullPath: string, srcPath: string, buildPath: string)
	local destPath = string.gsub(fullPath, srcPath, buildPath, 1)
	if fs.isFile(destPath) then
		fs.removeFile(destPath)
	end
end

local function generateBuildProject(
	srcPath: string,
	buildPath: string,
	defaultProjectPath: string?,
	buildProjectPath: string?
): (boolean, string?)
	local inputPath = defaultProjectPath or "default.project.json"
	local outputPath = buildProjectPath or "build.project.json"

	if not fs.isFile(inputPath) then
		return false, `Missing {inputPath} — run 'ezpm init' to generate it, or create one manually.`
	end

	local success, err = pcall(function()
		utils.modifyFileContents(inputPath, function(contents)
			return string.gsub(contents, srcPath .. "/", buildPath .. "/")
		end, outputPath)
	end)

	if not success then
		return false, `Failed to update {outputPath}: {err}`
	end

	return true, nil
end

local function cleanupOldBuild(buildPath: string): (boolean, string?)
	if not fs.isDir(buildPath) then
		return true, nil
	end

	local removeSuccess, removeErr = pcall(fs.removeDir, buildPath)
	if not removeSuccess then
		return false, `Failed to remove old build: {removeErr}`
	end

	return true, nil
end

local function createWatcherCallbacks(cfg: EzpmConfig)
	return {
		pattern = function(name: string): boolean
			return name:match("%.luau?$") ~= nil or name == "init.meta.json"
		end,

		onFileChanged = function(fullPath: string, _metadata)
			if fullPath:match("init%.meta%.json$") then
				utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Meta File Changed:", fullPath)
				local ok, err = pcall(copyMetaFile, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
				if not ok then
					warn("Error copying meta file after change:", err)
				end
				return
			end

			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Changed:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixSingleFile, fullPath, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file change:", err1)
			end

			local dirPath = utils.removeLastPathIdx(fullPath)
			local success2, err2 = pcall(darklua.formatPathAndExecute, dirPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if not success2 then
				warn("Error processing DarkLua after file change:", err2)
			end
		end,

		onFileCreated = function(fullPath: string, _metadata)
			if fullPath:match("init%.meta%.json$") then
				utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Meta File Created:", fullPath)
				local ok, err = pcall(copyMetaFile, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
				if not ok then
					warn("Error copying meta file after creation:", err)
				end
				return
			end

			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Created:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixSingleFile, fullPath, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file creation:", err1)
			end

			local success2, err2 = pcall(toolchain.generateSourceMap)
			if not success2 then
				warn("Error generating source map after file creation:", err2)
			end

			local success3, err3 =
				pcall(darklua.formatPathAndExecute, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH, true)
			if not success3 then
				warn("Error processing DarkLua after file creation:", err3)
			end
		end,

		onFileRemoved = function(fullPath: string)
			if fullPath:match("init%.meta%.json$") then
				utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Meta File Removed:", fullPath)
				local ok, err = pcall(removeMetaFile, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
				if not ok then
					warn("Error removing meta file:", err)
				end
				return
			end

			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Removed:", fullPath)
			local success1, err1 = pcall(toolchain.generateSourceMap)
			if not success1 then
				warn("Error generating source map after file removal:", err1)
			end

			local darkLuaPath = darklua.formatPathToDarkLua(fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if fs.isFile(darkLuaPath) then
				local success2, err2 = pcall(fs.removeFile, darkLuaPath)
				if not success2 then
					warn("Error removing DarkLua file:", err2)
				end
			end
			if fs.isDir(darkLuaPath) then
				local success3, err3 = pcall(fs.removeDir, darkLuaPath)
				if not success3 then
					warn("Error removing DarkLua directory:", err3)
				end
			end
		end,

		onDirectoryCreated = function(fullPath: string)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Directory Created:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after directory creation:", err1)
			end

			local success2, err2 = pcall(toolchain.generateSourceMap)
			if not success2 then
				warn("Error generating source map after directory creation:", err2)
			end

			local success3, err3 = pcall(darklua.formatPathAndExecute, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if not success3 then
				warn("Error processing DarkLua after directory creation:", err3)
			end
		end,

		onDirectoryRemoved = function(fullPath: string)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Directory Removed:", fullPath)
			local success1, err1 = pcall(toolchain.generateSourceMap)
			if not success1 then
				warn("Error generating source map after directory removal:", err1)
			end

			local darkLuaPath = darklua.formatPathToDarkLua(fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if fs.isDir(darkLuaPath) then
				local success2, err2 = pcall(fs.removeDir, darkLuaPath)
				if not success2 then
					warn("Error removing DarkLua directory:", err2)
				end
			else
				warn("Directory not found:", darkLuaPath)
			end
		end,
	}
end

local ROJO_DEFAULT_PORT = "34872"

local function parsePidFromLsof(output: string): string?
	local lines = string.split(output, "\n")
	for _, line in ipairs(lines) do
		local pid = line:match("rojo%s+(%d+)")
		if pid then
			return pid
		end
	end
	return nil
end

local function parsePidFromNetstat(output: string, port: string): string?
	local lines = string.split(output, "\n")
	for _, line in ipairs(lines) do
		local pid = line:match(":" .. port .. "%s+[%d%.%:]+%s+LISTENING%s+(%d+)")
		if pid then
			return pid
		end
	end
	return nil
end

local function findProcessOnPort(port: string): string?
	if process.os == "windows" then
		local ok, result = pcall(utils.executeWithOutput, "netstat", "-ano")
		if ok and result.ok then
			return parsePidFromNetstat(result.stdout, port)
		end
	else
		local ok, result = pcall(utils.executeWithOutput, "lsof", "-i", ":" .. port)
		if ok and result.ok and result.stdout:find("rojo") then
			return parsePidFromLsof(result.stdout)
		end
	end
	return nil
end

local function forceKillPid(pid: string): boolean
	local ok, result
	if process.os == "windows" then
		ok, result = pcall(utils.executeWithOutput, "taskkill", "/F", "/PID", pid)
	else
		ok, result = pcall(utils.executeWithOutput, "kill", "-9", pid)
	end
	return ok and result.ok
end

local function shutdownProcess(child: any)
	local killSuccess, killErr = pcall(function()
		child:kill()
	end)

	if killSuccess then
		task.wait(1.0)
	else
		warn("Failed to terminate Rojo process via Lune:", killErr)
		utils.log("Attempting direct system cleanup...")
	end

	local pid = findProcessOnPort(ROJO_DEFAULT_PORT)
	if not pid then
		utils.log("Rojo process terminated successfully")
		return
	end

	utils.log("Rojo still running on port " .. ROJO_DEFAULT_PORT .. " (PID: " .. pid .. ") - force killing...")
	if forceKillPid(pid) then
		task.wait(0.5)
		if not findProcessOnPort(ROJO_DEFAULT_PORT) then
			utils.log("Rojo process terminated successfully")
		else
			warn("Failed to terminate Rojo process - may need manual intervention")
		end
	else
		warn("Failed to force kill Rojo PID " .. pid)
	end
end

local function start(cfg: EzpmConfig)
	utils.log("Setting up Rojo serve...")

	utils.log("Updating build.project.json...")
	local buildOk, buildErr = generateBuildProject(cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
	if not buildOk then
		warn(buildErr)
		return
	end

	task.wait(0.5)

	utils.log("Removing old DarkLua build...")
	local cleanOk, cleanErr = cleanupOldBuild(cfg.DARKLUA_BUILD_PATH)
	if not cleanOk then
		warn(cleanErr)
		return
	end

	task.wait(0.5)

	utils.log("Generating source map...")
	local sourceMapResult = toolchain.generateSourceMap()
	if not sourceMapResult.ok then
		warn("Failed to generate source map:", sourceMapResult.stderr)
		return
	end
	task.wait(0.5)

	utils.log("Fixing require paths...")
	local pathFixSuccess, pathFixErr =
		pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
	if not pathFixSuccess then
		warn("Failed to fix require paths:", pathFixErr)
		return
	end
	task.wait(0.5)

	utils.log("Testing DarkLua build...")
	local darkLuaResult = darklua.process(cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH, true)
	if not darkLuaResult.ok or darkLuaResult.stderr ~= "" then
		warn("DarkLua build failed! Please fix the errors above and try again.")
		return
	end

	copyAllMetaFiles(cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
	utils.log("DarkLua build successful! Starting File-Watcher...")
	task.wait(0.5)

	local callbacks = createWatcherCallbacks(cfg)
	local startWatcher, stopWatcher = watcher.watchDirectory({
		path = cfg.SRC_PATH,
		pattern = callbacks.pattern,
		onFileChanged = callbacks.onFileChanged,
		onFileCreated = callbacks.onFileCreated,
		onFileRemoved = callbacks.onFileRemoved,
		onDirectoryCreated = callbacks.onDirectoryCreated,
		onDirectoryRemoved = callbacks.onDirectoryRemoved,
		checkInterval = 1,
	})

	utils.log("Serving Rojo and watching files...")

	local rojoChild = nil
	local rojoSuccess, rojoErr = pcall(function()
		rojoChild = process.create("rojo", { "serve", "build.project.json" }, { shell = true })
		utils.log("Rojo serve started successfully")
	end)

	if not rojoSuccess then
		warn("Failed to start Rojo serve:", rojoErr)
		return
	end

	local watcherRoutine = task.spawn(startWatcher)

	task.defer(function()
		task.wait()
		if cfg.DISPLAY_FILE_CHANGES then
			utils.log("")
			utils.log("| File-Watcher Active |")
			utils.log("Watching for changes in:", cfg.SRC_PATH)
		end
	end)

	print()
	stdio.prompt("text", "Press Enter to stop serving and watching...")
	stopWatcher()
	task.cancel(watcherRoutine)

	if rojoChild then
		shutdownProcess(rojoChild)
	end

	utils.log("Serve stopped.")
end

return {
	start = start,
	copyMetaFile = copyMetaFile,
	copyAllMetaFiles = copyAllMetaFiles,
	removeMetaFile = removeMetaFile,
	generateBuildProject = generateBuildProject,
	cleanupOldBuild = cleanupOldBuild,
	createWatcherCallbacks = createWatcherCallbacks,
	parsePidFromLsof = parsePidFromLsof,
	parsePidFromNetstat = parsePidFromNetstat,
	findProcessOnPort = findProcessOnPort,
	forceKillPid = forceKillPid,
	shutdownProcess = shutdownProcess,
}
