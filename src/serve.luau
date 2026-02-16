local fs = require("@lune/fs")
local stdio = require("@lune/stdio")
local task = require("@lune/task")

local watcher = require("./watcher")
local darklua = require("./darklua")
local requireFixer = require("./require-fixer")
local utils = require("./utils")
local configModule = require("./config")
local sync = require("./sync")

type EzpmConfig = configModule.EzpmConfig

local function start(cfg: EzpmConfig)
	local backendLabel = sync.getBackendLabel(cfg)
	local isAzulNative = cfg.SYNC.BACKEND == "azul" and cfg.SYNC.AZUL_MODE == "native"

	print("Setting up " .. backendLabel .. " serve...")

	-- Azul native mode doesn't need default.project.json or build.project.json
	if not isAzulNative then
		if not fs.isFile("default.project.json") then
			warn("Missing default.project.json — run 'ezpm init' to generate it, or create one manually.")
			return
		end

		print("Updating build.project.json...")
		local success, err = pcall(function()
			utils.modifyFileContents("default.project.json", function(contents)
				return string.gsub(contents, cfg.SRC_PATH .. "/", cfg.DARKLUA_BUILD_PATH .. "/")
			end, "build.project.json")
		end)

		if not success then
			warn("Failed to update build.project.json:", err)
			return
		end

		task.wait(0.5)
	end

	if fs.isDir(cfg.DARKLUA_BUILD_PATH) then
		print("Removing old DarkLua build...")
		local removeSuccess, removeErr = pcall(fs.removeDir, cfg.DARKLUA_BUILD_PATH)
		if not removeSuccess then
			warn("Failed to remove old build:", removeErr)
			return
		end

		task.wait(0.5)
	end

	print("Generating source map...")
	local sourceMapResult = sync.generateSourceMap(cfg)
	if not sourceMapResult.ok then
		warn("Failed to generate source map:", sourceMapResult.stderr)
		return
	end
	task.wait(0.5)

	print("Fixing require paths...")
	local pathFixSuccess, pathFixErr =
		pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
	if not pathFixSuccess then
		warn("Failed to fix require paths:", pathFixErr)
		return
	end
	task.wait(0.5)

	print("Testing DarkLua build...")
	local darkLuaResult = darklua.process(cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH, true)
	if not darkLuaResult.ok or darkLuaResult.stderr ~= "" then
		warn("DarkLua build failed! Please fix the errors above and try again.")
		return
	end
	print("DarkLua build successful! Starting File-Watcher...")
	task.wait(0.5)

	local function regenerateSourceMap()
		local smSuccess, smErr = pcall(sync.generateSourceMap, cfg)
		if not smSuccess then
			warn("Error generating source map:", smErr)
		end
	end

	local startWatcher, stopWatcher = watcher.watchDirectory({
		path = cfg.SRC_PATH,
		pattern = "%.luau?$",

		onFileChanged = function(fullPath: string, _metadata)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Changed:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file change:", err1)
			end

			regenerateSourceMap()

			local dirPath = utils.removeLastPathIdx(fullPath)
			local success3, err3 = pcall(darklua.formatPathAndExecute, dirPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if not success3 then
				warn("Error processing DarkLua after file change:", err3)
			end
		end,

		onFileCreated = function(fullPath: string, _metadata)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Created:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file creation:", err1)
			end

			regenerateSourceMap()

			local success3, err3 =
				pcall(darklua.formatPathAndExecute, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH, true)
			if not success3 then
				warn("Error processing DarkLua after file creation:", err3)
			end
		end,

		onFileRemoved = function(fullPath: string)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Removed:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file removal:", err1)
			end

			regenerateSourceMap()

			local darkLuaPath = darklua.formatPathToDarkLua(fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if fs.isFile(darkLuaPath) then
				local success3, err3 = pcall(fs.removeFile, darkLuaPath)
				if not success3 then
					warn("Error removing DarkLua file:", err3)
				end
			end
			if fs.isDir(darkLuaPath) then
				local success4, err4 = pcall(fs.removeDir, darkLuaPath)
				if not success4 then
					warn("Error removing DarkLua directory:", err4)
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

			regenerateSourceMap()

			local success3, err3 = pcall(darklua.formatPathAndExecute, fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if not success3 then
				warn("Error processing DarkLua after directory creation:", err3)
			end
		end,

		onDirectoryRemoved = function(fullPath: string)
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "Directory Removed:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after directory removal:", err1)
			end

			regenerateSourceMap()

			local darkLuaPath = darklua.formatPathToDarkLua(fullPath, cfg.SRC_PATH, cfg.DARKLUA_BUILD_PATH)
			if fs.isDir(darkLuaPath) then
				local success3, err3 = pcall(fs.removeDir, darkLuaPath)
				if not success3 then
					warn("Error removing DarkLua directory:", err3)
				end
			else
				warn("Directory not found:", darkLuaPath)
			end
		end,
		checkInterval = 1,
	})

	print("Serving " .. backendLabel .. " and watching files...")

	local syncProcess = sync.startSyncDaemon(cfg)
	if not syncProcess then
		return
	end

	local watcherRoutine = task.spawn(startWatcher)

	task.defer(function()
		task.wait()
		if cfg.DISPLAY_FILE_CHANGES then
			print("")
			print("| File-Watcher Active |")
			print("Watching for changes in:", cfg.SRC_PATH)
		end
	end)

	print()
	stdio.prompt("text", "Press Enter to stop serving and watching...")
	stopWatcher()
	task.cancel(watcherRoutine)

	sync.stopSyncDaemon(syncProcess, cfg)

	print("Serve stopped.")
end

return {
	start = start,
}
