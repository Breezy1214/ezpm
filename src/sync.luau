local process = require("@lune/process")
local task = require("@lune/task")

local utils = require("./utils")
local toolchain = require("./toolchain")
local sourcemap = require("./sourcemap")
local configModule = require("./config")

type EzpmConfig = configModule.EzpmConfig

export type SyncProcess = {
	child: any,
	backend: "rojo" | "azul",
	port: number?,
}

local function getBackendLabel(cfg: EzpmConfig): string
	if cfg.SYNC.BACKEND == "azul" then
		if cfg.SYNC.AZUL_MODE == "native" then
			return "Azul (native)"
		else
			return "Azul (rojo-compat)"
		end
	end
	return "Rojo"
end

local function generateSourceMap(cfg: EzpmConfig): utils.ExecResult
	if cfg.SYNC.BACKEND == "azul" then
		-- Use internal sourcemap generator (no rojo dependency)
		return sourcemap.generate("default.project.json", "sourcemap.json")
	else
		-- Use rojo sourcemap (original behavior)
		return toolchain.generateSourceMap()
	end
end

local function startSyncDaemon(cfg: EzpmConfig): SyncProcess?
	if cfg.SYNC.BACKEND == "azul" then
		if not toolchain.isAzulAvailable() then
			warn("Azul is not installed or not found in PATH.")
			print("Install it with: npm install -g azul")
			print("For more information, see: https://github.com/Ransomwave/azul")
			return nil
		end

		local args: { string }

		if cfg.SYNC.AZUL_MODE == "native" then
			local syncDir = cfg.SYNC.AZUL_SYNC_DIR
			if syncDir == "" then
				syncDir = cfg.DARKLUA_BUILD_PATH
			end
			args = {
				"--sync-dir=" .. syncDir,
				"--port=" .. tostring(cfg.SYNC.AZUL_PORT),
			}
		else
			-- rojo-compat mode
			args = {
				"--rojo",
				"--rojo-project=build.project.json",
				"--port=" .. tostring(cfg.SYNC.AZUL_PORT),
			}
		end

		local child = nil
		local startSuccess, startErr = pcall(function()
			child = process.create("azul", args)
			print(getBackendLabel(cfg) .. " sync started successfully")
		end)

		if not startSuccess then
			warn("Failed to start Azul:", startErr)
			return nil
		end

		return {
			child = child,
			backend = "azul",
			port = cfg.SYNC.AZUL_PORT,
		}
	else
		-- Rojo (default)
		local child = nil
		local startSuccess, startErr = pcall(function()
			local packagePath = toolchain.getPackagePath()
			child = process.create(packagePath .. "rojo", { "serve", "build.project.json" })
			print("Rojo serve started successfully")
		end)

		if not startSuccess then
			warn("Failed to start Rojo serve:", startErr)
			return nil
		end

		return {
			child = child,
			backend = "rojo",
			port = 34872,
		}
	end
end

local function stopSyncDaemon(syncProcess: SyncProcess?, cfg: EzpmConfig)
	if not syncProcess or not syncProcess.child then
		return
	end

	local isMacOS = process.os == "macos"
	local backendName = if syncProcess.backend == "azul" then "Azul" else "Rojo"
	local port = syncProcess.port or (if syncProcess.backend == "azul" then cfg.SYNC.AZUL_PORT else 34872)

	local killSuccess, killErr = pcall(function()
		syncProcess.child:kill()
	end)

	if killSuccess then
		task.wait(1.0)
		if not isMacOS then
			return
		end

		-- Check if process is still running on the port (macOS only)
		local processName = string.lower(backendName)
		local checkResult = utils.executeWithOutput("lsof", "-i", ":" .. tostring(port))
		local isStillRunning = checkResult.ok and checkResult.stdout:find(processName)

		if not isStillRunning then
			print(backendName .. " process terminated successfully")
		else
			local lines = string.split(checkResult.stdout, "\n")
			for _, line in ipairs(lines) do
				local pid = line:match(processName .. "%s+(%d+)")
				if pid then
					print("Force killing " .. backendName .. " PID:", pid)
					local forceKillResult = utils.executeWithOutput("kill", "-9", pid)
					if forceKillResult.ok then
						task.wait(0.5)
						local verifyResult = utils.executeWithOutput("lsof", "-i", ":" .. tostring(port))
						if not (verifyResult.ok and verifyResult.stdout:find(processName)) then
							print(backendName .. " process successfully terminated")
						else
							warn("Failed to terminate " .. backendName .. " process - may need manual intervention")
						end
					else
						warn("Failed to force kill " .. backendName .. " process:", forceKillResult.stderr)
					end
					break
				end
			end
		end
	else
		warn("Failed to terminate " .. backendName .. " process via Lune:", killErr)
		print("Attempting direct system cleanup...")

		if not isMacOS then
			return
		end

		local processName = string.lower(backendName)
		local checkResult = utils.executeWithOutput("lsof", "-i", ":" .. tostring(port))
		if checkResult.ok and checkResult.stdout:find(processName) then
			local lines = string.split(checkResult.stdout, "\n")
			for _, line in ipairs(lines) do
				local pid = line:match(processName .. "%s+(%d+)")
				if pid then
					print("Found " .. backendName .. " PID:", pid, "- terminating...")
					local killResult = utils.executeWithOutput("kill", "-9", pid)
					if killResult.ok then
						print(backendName .. " process terminated successfully")
					else
						warn("Failed to kill " .. backendName .. " PID", pid .. ":", killResult.stderr)
					end
				end
			end
		else
			print("No " .. backendName .. " processes found on port " .. tostring(port))
		end
	end
end

return {
	getBackendLabel = getBackendLabel,
	generateSourceMap = generateSourceMap,
	startSyncDaemon = startSyncDaemon,
	stopSyncDaemon = stopSyncDaemon,
}
