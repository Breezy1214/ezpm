local fs = require("@lune/fs")
local process = require("@lune/process")
local stdio = require("@lune/stdio")

export type ExecResult = {
	ok: boolean,
	code: number,
	stdout: string,
	stderr: string,
}

local function getCurrentTime()
	local currentTime = os.date("*t")
	local hour = currentTime.hour
	local minute = currentTime.min
	local second = currentTime.sec
	local milliseconds = math.floor((os.clock() % 1) * 1000)
	return string.format("%02d:%02d:%02d:%03d", hour, minute, second, milliseconds)
end

local function execute(...): ExecResult
	local tbl = { ... }
	local program = table.remove(tbl, 1)
	return process.exec(program, tbl, {
		stdio = {
			stdout = "inherit",
			stderr = "inherit",
		},
	})
end

local function executeString(command: string): ExecResult
	return execute(table.unpack(string.split(command, " ")))
end

local function executeWithOutput(...): ExecResult
	local tbl = { ... }
	local program = table.remove(tbl, 1)
	return process.exec(program, tbl, {
		stdio = {
			stdout = "default",
			stderr = "default",
		},
	})
end

local function modifyFileContents(
	readFilePath: string,
	modifyFunc: (contents: string) -> string,
	writeFilePath: string?
)
	local contents = fs.readFile(readFilePath)
	local modifiedContents = modifyFunc(contents)
	fs.writeFile(writeFilePath or readFilePath, modifiedContents)
end

local function removeLastPathIdx(path: string): string
	local pathToArray = string.split(path, "/")
	table.remove(pathToArray, #pathToArray)
	return table.concat(pathToArray, "/")
end

local function printChanges(displayEnabled: boolean, ...)
	if displayEnabled then
		print(getCurrentTime(), "-", ...)
	end
end

local function resetStyle()
	stdio.write(stdio.style("reset"))
end

return {
	getCurrentTime = getCurrentTime,
	execute = execute,
	executeString = executeString,
	executeWithOutput = executeWithOutput,
	modifyFileContents = modifyFileContents,
	removeLastPathIdx = removeLastPathIdx,
	printChanges = printChanges,
	resetStyle = resetStyle,
}
