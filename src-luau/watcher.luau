local fs = require("@lune/fs")
local task = require("@lune/task")

export type FileWatchOptions = {
	path: string?,
	pattern: (string | (string) -> boolean)?,
	onFileCreated: (fullPath: string, metadata: any) -> (),
	onFileChanged: (fullPath: string, metadata: any) -> (),
	onFileRemoved: (fullPath: string) -> (),
	onDirectoryCreated: (fullPath: string) -> (),
	onDirectoryRemoved: (fullPath: string) -> (),
	checkInterval: number?,
}

local function scanDirectory(dir: string, pattern: string | (string) -> boolean)
	local files = {}
	local directories = {}

	local function matchesPattern(entry: string): boolean
		if type(pattern) == "function" then
			return pattern(entry) == true
		else
			return entry:match(pattern) ~= nil
		end
	end

	local function scanRecursive(path)
		local success, entries = pcall(fs.readDir, path)
		if not success then
			warn("Failed to read directory:", path)
			return
		end

		for _, entry in ipairs(entries) do
			local fullPath = `{path}/{entry}`

			local isDirectory = false
			local isFile = false

			local dirSuccess, dirResult = pcall(fs.isDir, fullPath)
			if dirSuccess then
				isDirectory = dirResult
			end

			local fileSuccess, fileResult = pcall(fs.isFile, fullPath)
			if fileSuccess then
				isFile = fileResult
			end

			if isDirectory then
				directories[fullPath] = true
				scanRecursive(fullPath)
			elseif isFile and matchesPattern(entry) then
				local metaSuccess, metadata = pcall(fs.metadata, fullPath)
				if metaSuccess then
					files[fullPath] = metadata
				else
					warn("Failed to get metadata for:", fullPath)
				end
			end
		end
	end

	scanRecursive(dir)
	return files, directories
end

local function watchDirectory(options: FileWatchOptions)
	local dir = options.path or "."
	local pattern = options.pattern or ".*"
	local checkInterval = options.checkInterval or 0

	local success, isDir = pcall(fs.isDir, dir)
	if not success or not isDir then
		error(`Directory does not exist or cannot be accessed: {dir}`)
	end

	local fileStates, directoryStates = scanDirectory(dir, pattern)
	local watcherActive = true

	local function safeExecute(callback: any, ...)
		if not callback then
			return
		end
		local success, err = pcall(callback, ...)
		if not success then
			warn(`File watcher callback error: {err}`)
			-- Continue watching even if callback fails
		end
	end

	local function updateStates()
		if not watcherActive then
			return
		end

		local success, currentFiles, currentDirectories = pcall(scanDirectory, dir, pattern)
		if not success then
			warn(`Failed to scan directory {dir}: {currentFiles}`)
			return
		end

		-- Check for new and modified files
		for fullPath, metadata in currentFiles do
			if not fileStates[fullPath] then
				safeExecute(options.onFileCreated :: any, fullPath, metadata)
			elseif fileStates[fullPath] and metadata.modifiedAt ~= fileStates[fullPath].modifiedAt then
				safeExecute(options.onFileChanged :: any, fullPath, metadata)
			end
		end

		-- Check for removed files
		for fullPath, _ in fileStates do
			if not currentFiles[fullPath] then
				safeExecute(options.onFileRemoved :: any, fullPath)
			end
		end

		-- Check for new directories
		for fullPath, _ in currentDirectories do
			if not directoryStates[fullPath] then
				safeExecute(options.onDirectoryCreated :: any, fullPath)
			end
		end

		-- Check for removed directories
		for fullPath, _ in directoryStates do
			if not currentDirectories[fullPath] then
				safeExecute(options.onDirectoryRemoved :: any, fullPath)
			end
		end

		fileStates = currentFiles
		directoryStates = currentDirectories
	end

	local function stopWatcher()
		watcherActive = false
	end

	local function startWatcher()
		return function()
			while watcherActive do
				local success, err = pcall(updateStates)
				if not success then
					warn(`File watcher error: {err}`) -- Continue watching despite errors
				end

				if checkInterval > 0 then
					task.wait(checkInterval)
				else
					task.wait() -- Wait for the next frame
				end
			end
		end
	end

	return startWatcher(), stopWatcher
end

return {
	scanDirectory = scanDirectory,
	watchDirectory = watchDirectory,
}
