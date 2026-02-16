local fs = require("@lune/fs")
local serde = require("@lune/serde")

local utils = require("./utils")

-- Roblox service classNames that map to themselves
local ROBLOX_SERVICES: { [string]: boolean } = {
	Workspace = true,
	ReplicatedFirst = true,
	ReplicatedStorage = true,
	ServerScriptService = true,
	ServerStorage = true,
	StarterGui = true,
	StarterPack = true,
	StarterPlayer = true,
	StarterPlayerScripts = true,
	StarterCharacterScripts = true,
	SoundService = true,
	Chat = true,
	LocalizationService = true,
	TestService = true,
	Lighting = true,
	MaterialService = true,
	Teams = true,
}

type SourcemapNode = {
	name: string,
	className: string,
	filePaths: { string }?,
	children: { SourcemapNode }?,
}

local function classifyFile(fileName: string): (string, string)?
	-- init files make the parent directory a ModuleScript
	if fileName == "init.luau" or fileName == "init.lua" then
		return nil
	end

	if fileName:match("%.server%.luau$") or fileName:match("%.server%.lua$") then
		local name = fileName:gsub("%.server%.luau$", ""):gsub("%.server%.lua$", "")
		return name, "Script"
	end

	if fileName:match("%.client%.luau$") or fileName:match("%.client%.lua$") then
		local name = fileName:gsub("%.client%.luau$", ""):gsub("%.client%.lua$", "")
		return name, "LocalScript"
	end

	if fileName:match("%.luau$") or fileName:match("%.lua$") then
		local name = fileName:gsub("%.luau$", ""):gsub("%.lua$", "")
		return name, "ModuleScript"
	end

	return nil
end

local function hasInitFile(dirPath: string): boolean
	return fs.isFile(dirPath .. "/init.luau") or fs.isFile(dirPath .. "/init.lua")
end

local function getInitFilePath(dirPath: string): string?
	if fs.isFile(dirPath .. "/init.luau") then
		return dirPath .. "/init.luau"
	elseif fs.isFile(dirPath .. "/init.lua") then
		return dirPath .. "/init.lua"
	end
	
	return nil
end

local function scanDirectoryToNodes(dirPath: string): { SourcemapNode }
	local children: { SourcemapNode } = {}

	local success, entries = pcall(fs.readDir, dirPath)
	if not success then
		return children
	end

	table.sort(entries)

	for _, entry in entries do
		local fullPath = dirPath .. "/" .. entry

		if fs.isDir(fullPath) then
			local subChildren = scanDirectoryToNodes(fullPath)
			local node: SourcemapNode

			if hasInitFile(fullPath) then
				local initPath = getInitFilePath(fullPath)
				node = {
					name = entry,
					className = "ModuleScript",
					filePaths = if initPath then { initPath } else nil,
					children = if #subChildren > 0 then subChildren else nil,
				}
			else
				node = {
					name = entry,
					className = "Folder",
					children = if #subChildren > 0 then subChildren else nil,
				}
			end

			table.insert(children, node)
		elseif fs.isFile(fullPath) then
			local name, className = classifyFile(entry)
			if name then
				local node: SourcemapNode = {
					name = name,
					className = className,
					filePaths = { fullPath },
				}
				table.insert(children, node)
			end
		end
	end

	return children
end

local function buildNodeFromProjectTree(name: string, treeNode: any, parentPath: string?): SourcemapNode
	local className = treeNode["$className"]
	local path = treeNode["$path"]

	local node: SourcemapNode = {
		name = name,
		className = className or (if ROBLOX_SERVICES[name] then name else "Folder"),
	}

	-- If this node has a $path, scan that directory
	if path then
		if fs.isDir(path) then
			local scannedChildren = scanDirectoryToNodes(path)

			if hasInitFile(path) then
				local initPath = getInitFilePath(path)
				node.className = "ModuleScript"
				node.filePaths = if initPath then { initPath } else nil
			end

			if #scannedChildren > 0 then
				node.children = scannedChildren
			end
		elseif fs.isFile(path) then
			local _, fileClass = classifyFile(path:match("([^/]+)$") or path)
			if fileClass then
				node.className = fileClass
			end
			node.filePaths = { path }
		end
	end

	-- Process child tree nodes (non-$ keys)
	for key, value in treeNode do
		if type(key) == "string" and key:sub(1, 1) ~= "$" and type(value) == "table" then
			local childNode = buildNodeFromProjectTree(key, value, path)

			if not node.children then
				node.children = {}
			end
			table.insert(node.children, childNode)
		end
	end

	return node
end

local function generate(projectJsonPath: string, outputPath: string): utils.ExecResult
	if not fs.isFile(projectJsonPath) then
		local errMsg = "Project file not found: " .. projectJsonPath
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local readSuccess, contents = pcall(fs.readFile, projectJsonPath)
	if not readSuccess then
		local errMsg = "Failed to read project file: " .. projectJsonPath
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local parseSuccess, projectJson = pcall(serde.decode, "json", contents)
	if not parseSuccess then
		local errMsg = "Failed to parse project JSON: " .. projectJsonPath
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local tree = projectJson.tree
	if not tree then
		local errMsg = "No 'tree' field in project JSON"
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local projectName = projectJson.name or "Game"
	local rootNode = buildNodeFromProjectTree(projectName, tree)

	local encodeSuccess, jsonOutput = pcall(serde.encode, "json", rootNode, true)
	if not encodeSuccess then
		local errMsg = "Failed to encode sourcemap JSON"
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	local writeSuccess, writeErr = pcall(fs.writeFile, outputPath, jsonOutput)
	if not writeSuccess then
		local errMsg = "Failed to write sourcemap: " .. tostring(writeErr)
		warn(errMsg)
		return { ok = false, code = 1, stdout = "", stderr = errMsg }
	end

	return { ok = true, code = 0, stdout = "", stderr = "" }
end

return {
	generate = generate,
}
