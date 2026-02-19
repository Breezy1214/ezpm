local net = require("@lune/net")
local stdio = require("@lune/stdio")
local serde = require("@lune/serde")

local utils = require("@self/utils")

local EZPM_VERSION = "0.1.3"

type SemVer = {
	major: number,
	minor: number,
	patch: number,
}

local function parseSemver(version: string): SemVer?
	local cleaned = version:gsub("^v", "")
	local major, minor, patch = cleaned:match("^(%d+)%.(%d+)%.(%d+)")
	if not major then
		return nil
	end
	return {
		major = tonumber(major) :: number,
		minor = tonumber(minor) :: number,
		patch = tonumber(patch) :: number,
	}
end

local function isOlderThan(a: SemVer, b: SemVer): boolean
	if a.major ~= b.major then
		return a.major < b.major
	end
	if a.minor ~= b.minor then
		return a.minor < b.minor
	end
	return a.patch < b.patch
end

local function checkForUpdates()
	local response = net.request({
		url = "https://api.github.com/repos/Breezy1214/sync/releases/latest",
		method = "GET",
		headers = {
			Accept = "application/vnd.github.v3+json",
		},
	})

	if not response.ok then
		return
	end

	local data = serde.decode("json", response.body)
	local tagName = data.tag_name
	if not tagName then
		return
	end

	local current = parseSemver(EZPM_VERSION)
	local latest = parseSemver(tagName)
	if not current or not latest then
		return
	end

	if isOlderThan(current, latest) then
		local latestStr = `{latest.major}.{latest.minor}.{latest.patch}`
		print("")
		stdio.write(stdio.style("bold"))
		stdio.write(stdio.color("yellow"))
		stdio.write("UPDATE AVAILABLE")
		utils.resetStyle()
		print(`  ezpm {EZPM_VERSION} -> {latestStr}`)
		utils.printDim("  Run: rokit update ezpm")
		utils.printDim("       rokit install")
		utils.printDim("  Or download: https://github.com/Breezy1214/sync/releases/latest")
		print("")
	end
end

return {
	VERSION = EZPM_VERSION,
	parseSemver = parseSemver,
	isOlderThan = isOlderThan,
	checkForUpdates = checkForUpdates,
}
