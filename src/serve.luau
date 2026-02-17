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

local function start(cfg: EzpmConfig)
	print("Setting up Rojo serve...")

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
	local sourceMapResult = toolchain.generateSourceMap()
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

			local success2, err2 = pcall(toolchain.generateSourceMap)
			if not success2 then
				warn("Error generating source map after file change:", err2)
			end

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
			utils.printChanges(cfg.DISPLAY_FILE_CHANGES, "File Removed:", fullPath)
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after file removal:", err1)
			end

			local success2, err2 = pcall(toolchain.generateSourceMap)
			if not success2 then
				warn("Error generating source map after file removal:", err2)
			end

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
			local success1, err1 =
				pcall(requireFixer.fixRequirePaths, cfg.SRC_PATH, cfg.DISPLAY_FILE_CHANGES, cfg.IGNORE_LIST)
			if not success1 then
				warn("Error fixing paths after directory removal:", err1)
			end

			local success2, err2 = pcall(toolchain.generateSourceMap)
			if not success2 then
				warn("Error generating source map after directory removal:", err2)
			end

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

	print("Serving Rojo and watching files...")

	local rojoChild = nil
	local rojoSuccess, rojoErr = pcall(function()
		rojoChild = process.create("rojo", { "serve", "build.project.json" }, { shell = true })
		print("Rojo serve started successfully")
	end)

	if not rojoSuccess then
		warn("Failed to start Rojo serve:", rojoErr)
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

	local isMacOS = process.os == "macos"
	print()
	stdio.prompt("text", "Press Enter to stop serving and watching...")
	stopWatcher()
	task.cancel(watcherRoutine)

	if rojoChild then
		local killSuccess, killErr = pcall(function()
			rojoChild:kill()
		end)

		if killSuccess then
			task.wait(1.0)
			if not isMacOS then
				return
			end

			-- Check if rojo is still running on the port
			local checkResult = utils.executeWithOutput("lsof", "-i", ":34872")
			local isStillRunning = checkResult.ok and checkResult.stdout:find("rojo")

			if not isStillRunning then
				print("Rojo process terminated successfully")
			else
				-- Extract PID from lsof output and force kill it
				local lines = string.split(checkResult.stdout, "\n")
				for _, line in ipairs(lines) do
					local pid = line:match("rojo%s+(%d+)")
					if pid then
						print("Force killing Rojo PID:", pid)
						local forceKillResult = utils.executeWithOutput("kill", "-9", pid)
						if forceKillResult.ok then
							task.wait(0.5)
							local verifyResult = utils.executeWithOutput("lsof", "-i", ":34872")
							if not (verifyResult.ok and verifyResult.stdout:find("rojo")) then
								print("Rojo process successfully terminated")
							else
								warn("Failed to terminate Rojo process - may need manual intervention")
							end
						else
							warn("Failed to force kill Rojo process:", forceKillResult.stderr)
						end
						break
					end
				end
			end
		else
			warn("Failed to terminate Rojo process via Lune:", killErr)
			print("Attempting direct system cleanup...")

			if not isMacOS then
				return
			end

			-- Fallback: Find and kill rojo processes directly
			local checkResult = utils.executeWithOutput("lsof", "-i", ":34872")
			if checkResult.ok and checkResult.stdout:find("rojo") then
				local lines = string.split(checkResult.stdout, "\n")
				for _, line in ipairs(lines) do
					local pid = line:match("rojo%s+(%d+)")
					if pid then
						print("Found Rojo PID:", pid, "- terminating...")
						local killResult = utils.executeWithOutput("kill", "-9", pid)
						if killResult.ok then
							print("Rojo process terminated successfully")
						else
							warn("Failed to kill Rojo PID", pid .. ":", killResult.stderr)
						end
					end
				end
			else
				print("No Rojo processes found on port 34872")
			end
		end
	end

	print("Serve stopped.")
end

return {
	start = start,
}
