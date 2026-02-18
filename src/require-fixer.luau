local fs = require("@lune/fs")

local darklua = require("./darklua")

local function findFileByName(expectedFileName, searchDir)
	local items = fs.readDir(searchDir)

	for _, item in ipairs(items) do
		if item == expectedFileName or item == expectedFileName .. ".luau" or item == expectedFileName .. ".lua" then
			local path = searchDir .. "/" .. item
			if fs.isDir(path) then
				return path .. "/" .. "init.luau"
			end
			return path
		end

		local itemPath = searchDir .. "/" .. item
		if fs.isDir(itemPath) then
			local found = findFileByName(expectedFileName, itemPath)
			if found then
				return found
			end
		end
	end

	return nil
end

local function processSingleFile(
	filePath: string,
	rootDir: string,
	displayFileChanges: boolean,
	ignoreList: { string },
	shortcuts: { [string]: string },
	sortedInvertedShortcuts: { { realPath: string, shortcut: string } },
	sortedSrcShortcuts: { { shortcut: string, realPath: string } }
): boolean
	local content = fs.readFile(filePath)

	if content then
		local didMakeChange = false

		-- Look for require("path/to/file") patterns
		for requiredPath in content:gmatch('require%("(.-)"%)') do
			local shouldIgnore = false

			for _, ignorePath in ignoreList do
				if requiredPath:find(ignorePath) then
					shouldIgnore = true
					break
				end
			end

			if shouldIgnore then
				continue
			end

			-- Skip built-in Roblox require aliases (@self, @game)
			local lowerPath = requiredPath:lower()
			if
				lowerPath:match("^@self/")
				or lowerPath:match("^@self$")
				or lowerPath:match("^@game/")
				or lowerPath:match("^@game$")
			then
				continue
			end

			-- Handle direct src/ paths that need to be converted to aliases
			local newRequirePath = requiredPath
			local foundMatch = false

			-- Try to match against pre-sorted configured shortcuts
			for _, entry in ipairs(sortedSrcShortcuts) do
				local pattern = "^" .. entry.realPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1")

				if requiredPath:match(pattern) then
					newRequirePath = requiredPath:gsub(pattern, entry.shortcut)
					content = content:gsub(
						'require%("' .. requiredPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1") .. '"%)',
						'require("' .. newRequirePath .. '")'
					)
					didMakeChange = true
					foundMatch = true
					if displayFileChanges then
						print(
							"  ✓ Fixed source path in "
								.. filePath:gsub(rootDir .. "/", "")
								.. " | "
								.. requiredPath
								.. " -> "
								.. newRequirePath
						)
					end
					break
				end
			end

			if foundMatch then
				continue
			elseif requiredPath:match("^" .. rootDir .. "/") then
				-- Handle other src/ paths by searching for the file
				local expectedFileName = requiredPath:match("([^/]+)$")
				local newFilePath = findFileByName(expectedFileName, rootDir)

				if newFilePath then
					local convertedPath = newFilePath
					for _, entry in sortedInvertedShortcuts do
						convertedPath = convertedPath:gsub(entry.realPath, entry.shortcut)
					end
					convertedPath = convertedPath:gsub("/init", ""):gsub("%.lua$", ""):gsub("%.luau$", "")

					content = content:gsub(
						'require%("' .. requiredPath:gsub("([%(%)%.%+%-%*%?%[%]%^%$%%])", "%%%1") .. '"%)',
						'require("' .. convertedPath .. '")'
					)

					didMakeChange = true
					if displayFileChanges then
						print(
							"  ✓ Fixed source path in "
								.. filePath:gsub(rootDir .. "/", "")
								.. " | "
								.. requiredPath
								.. " -> "
								.. convertedPath
						)
					end

					continue
				else
					warn("Could not find file for source path: " .. requiredPath .. " in file: " .. filePath)
					continue
				end
			end

			-- Apply shortcut resolution if necessary
			local resolvedPath = requiredPath
			for shortcut, realPath in shortcuts do
				if requiredPath:sub(1, #shortcut) == shortcut then
					resolvedPath = realPath .. requiredPath:sub(#shortcut + 1)
					break
				end
			end

			if resolvedPath:find("%.%./") then
				warn(
					"../ paths are not supported currently. Please fix the path in file: " .. filePath,
					"| Path to fix: " .. requiredPath
				)
				continue
			elseif resolvedPath:find("%./") then
				warn(
					"./ paths are not supported currently. Please fix the path in file: " .. filePath,
					"| Path to fix: " .. requiredPath
				)
				continue
			end

			-- Convert Lua-style paths to file paths
			local requiredFilePath = resolvedPath:gsub("%.", "/"):gsub(".luau$", ""):gsub(".lua$", "")
			local fullPath = requiredFilePath .. ".luau"

			-- If the file does not exist, attempt to find it
			if not fs.isFile(fullPath) and not fs.isFile(requiredFilePath .. "/init.luau") then
				local expectedFileName = requiredFilePath:match("([^/]+)$")
				local newFilePath = findFileByName(expectedFileName, rootDir)

				if newFilePath then
					-- Replace the old path with the new one
					local convertedRequirePath = newFilePath
					for _, entry in sortedInvertedShortcuts do
						convertedRequirePath = convertedRequirePath:gsub(entry.realPath, entry.shortcut)
					end

					convertedRequirePath = convertedRequirePath:gsub("/init", ""):gsub("%.lua$", ""):gsub("%.luau$", "")

					if requiredPath == convertedRequirePath then
						continue
					end

					content =
						content:gsub('require%("' .. requiredPath .. '"%)', 'require("' .. convertedRequirePath .. '")')

					didMakeChange = true
					if displayFileChanges then
						print(
							"  ✓ Fixed require path in "
								.. filePath:gsub(rootDir .. "/", "")
								.. " | "
								.. requiredPath
								.. " -> "
								.. convertedRequirePath
						)
					end
				else
					warn(
						"Issue found in file: "
							.. filePath
							.. ". Could not find file '"
							.. expectedFileName
							.. "' for require path: "
							.. requiredPath
					)
				end
			end
		end

		if didMakeChange then
			fs.writeFile(filePath, content)
		end

		return didMakeChange
	end

	return false
end

local function fixSingleFile(
	filePath: string,
	rootDir: string,
	displayFileChanges: boolean,
	ignoreList: { string }
): boolean
	local shortcuts = darklua.loadShortcuts()
	local sortedInvertedShortcuts = darklua.getSortedInvertedShortcuts()
	local sortedSrcShortcuts = darklua.getSortedSrcShortcuts(rootDir)
	return processSingleFile(
		filePath,
		rootDir,
		displayFileChanges,
		ignoreList,
		shortcuts,
		sortedInvertedShortcuts,
		sortedSrcShortcuts
	)
end

local function fixRequirePaths(rootDir: string, displayFileChanges: boolean, ignoreList: { string }): boolean
	print("Fixing require paths in:", rootDir)

	local success, isDir = pcall(fs.isDir, rootDir)
	if not success or not isDir then
		warn("Directory does not exist or cannot be accessed:", rootDir)
		return false
	end

	local shortcuts = darklua.loadShortcuts()
	local sortedInvertedShortcuts = darklua.getSortedInvertedShortcuts()
	local sortedSrcShortcuts = darklua.getSortedSrcShortcuts(rootDir)

	local function collectFiles(dir)
		local files = {}
		local items = fs.readDir(dir)

		for _, item in ipairs(items) do
			local itemPath = dir .. "/" .. item
			if fs.isDir(itemPath) then
				local subFiles = collectFiles(itemPath)
				for _, subFile in ipairs(subFiles) do
					table.insert(files, subFile)
				end
			elseif fs.isFile(itemPath) and (item:match("%.luau$") or item:match("%.lua$") or item == "init.luau") then
				table.insert(files, itemPath)
			end
		end

		return files
	end

	local files = collectFiles(rootDir)
	local fixedCount = 0

	for _, filePath in ipairs(files) do
		if
			processSingleFile(
				filePath,
				rootDir,
				displayFileChanges,
				ignoreList,
				shortcuts,
				sortedInvertedShortcuts,
				sortedSrcShortcuts
			)
		then
			fixedCount = fixedCount + 1
		end
	end

	if fixedCount > 0 then
		print("✓ Fixed require paths in", fixedCount, "files.")
	end
	return true
end

return {
	fixRequirePaths = fixRequirePaths,
	fixSingleFile = fixSingleFile,
}
