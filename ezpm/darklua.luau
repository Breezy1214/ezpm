local fs = require("@lune/fs")
local serde = require("@lune/serde")

local utils = require("./utils")

local SHORTCUTS: { [string]: string }? = nil
local INVERTED_SHORTCUTS: { [string]: string }? = nil
local SORTED_INVERTED_SHORTCUTS: { { realPath: string, shortcut: string } }? = nil
local SORTED_SRC_SHORTCUTS: { { shortcut: string, realPath: string } }? = nil

local function loadShortcuts(): { [string]: string }
	if SHORTCUTS then
		return SHORTCUTS
	end

	local shortcuts = {}

	if not fs.isFile(".darklua.json") then
		warn("No .darklua.json file found. Please ensure you have run DarkLua to generate it.")
		SHORTCUTS = shortcuts
		return shortcuts
	end

	local success, darkluaContent = pcall(fs.readFile, ".darklua.json")
	if not success then
		warn("Failed to read .darklua.json")
		SHORTCUTS = shortcuts
		return shortcuts
	end

	local parseSuccess, darkluaConfig = pcall(serde.decode, "json", darkluaContent)
	if not parseSuccess then
		warn("Failed to parse .darklua.json")
		SHORTCUTS = shortcuts
		return shortcuts
	end

	local sources = darkluaConfig
		and darkluaConfig.process
		and darkluaConfig.process[1]
		and darkluaConfig.process[1].current
		and darkluaConfig.process[1].current.sources

	if sources then
		-- Convert sources to shortcuts format (add trailing slash)
		for alias, path in sources do
			shortcuts[alias .. "/"] = path
		end
	else
		warn("Failed to parse .darklua.json or missing sources configuration")
	end

	SHORTCUTS = shortcuts
	return shortcuts
end

local function getInvertedShortcuts(): { [string]: string }
	if INVERTED_SHORTCUTS then
		return INVERTED_SHORTCUTS
	end

	local shortcuts = loadShortcuts()
	local inverted = {}
	for k, v in shortcuts do
		inverted[v] = k
	end
	INVERTED_SHORTCUTS = inverted
	return inverted
end

local function getSortedInvertedShortcuts(): { { realPath: string, shortcut: string } }
	if SORTED_INVERTED_SHORTCUTS then
		return SORTED_INVERTED_SHORTCUTS
	end

	local inverted = getInvertedShortcuts()
	local sorted = {}
	for realPath, shortcut in inverted do
		table.insert(sorted, { realPath = realPath, shortcut = shortcut })
	end
	table.sort(sorted, function(a, b)
		return #a.realPath > #b.realPath
	end)
	SORTED_INVERTED_SHORTCUTS = sorted
	return sorted
end

local function getSortedSrcShortcuts(): { { shortcut: string, realPath: string } }
	if SORTED_SRC_SHORTCUTS then
		return SORTED_SRC_SHORTCUTS
	end

	local shortcuts = loadShortcuts()
	local sorted = {}
	for shortcut, realPath in shortcuts do
		-- Only process shortcuts that map to src/ paths
		if realPath:match("^src/") then
			table.insert(sorted, { shortcut = shortcut, realPath = realPath })
		end
	end

	table.sort(sorted, function(a, b)
		return #a.realPath > #b.realPath
	end)

	SORTED_SRC_SHORTCUTS = sorted
	return sorted
end

local function process(
	srcPath: string?,
	buildPath: string?,
	printResults: boolean?,
	dontRetry: boolean?
): utils.ExecResult
	srcPath = srcPath or "src"
	buildPath = buildPath or "darklua_build"

	-- Validate input path exists
	if not fs.isDir(srcPath) then
		local errMsg = `Input path does not exist: {srcPath}`
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local results = utils.executeWithOutput("darklua", "process", srcPath, buildPath)

	if not results.ok then
		warn("DarkLua process failed with exit code", results.code)
		if results.stderr ~= "" then
			warn("DarkLua Error:", results.stderr)
		end
		
		if results.stdout ~= "" then
			warn("DarkLua Output:", results.stdout)
		end
	elseif results.stderr ~= "" then
		warn("DarkLua Warning:", results.stderr)
		if not dontRetry then
			print("Build warning detected. Trying to rebuild DarkLua...")
			return process(nil, nil, true, true)
		end
	elseif printResults and results.stdout ~= "" then
		print("DarkLua Success:", results.stdout)
	end

	return results
end

local function formatPathToDarkLua(path: string, srcPath: string, buildPath: string, removeLast: boolean?): string
	local pathToArray = string.split(path, "/")
	if removeLast then
		table.remove(pathToArray, #pathToArray)
	end
	path = table.concat(pathToArray, "/")
	return string.gsub(path, srcPath, buildPath)
end

local function formatPathAndExecute(path: string, srcPath: string, buildPath: string, removeLast: boolean?)
	local darkLuaPath = formatPathToDarkLua(path, srcPath, buildPath, removeLast)
	return process(path, darkLuaPath)
end

return {
	loadShortcuts = loadShortcuts,
	getInvertedShortcuts = getInvertedShortcuts,
	getSortedInvertedShortcuts = getSortedInvertedShortcuts,
	getSortedSrcShortcuts = getSortedSrcShortcuts,
	process = process,
	formatPathToDarkLua = formatPathToDarkLua,
	formatPathAndExecute = formatPathAndExecute,
}
