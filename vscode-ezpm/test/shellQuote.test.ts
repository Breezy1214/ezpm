import { strict as assert } from "node:assert";
import { quoteForShell } from "../src/core/shellQuote";

describe("shellQuote", () => {
	it("quotes POSIX shell arguments with spaces", () => {
		assert.equal(
			quoteForShell("/Users/example/My Tools/ezpm", "darwin"),
			"'/Users/example/My Tools/ezpm'",
		);
	});

	it("escapes embedded POSIX single quotes", () => {
		assert.equal(quoteForShell("it's-ready", "linux"), "'it'\\''s-ready'");
	});

	it("quotes Windows shell arguments", () => {
		assert.equal(
			quoteForShell("C:\\Program Files\\ezpm\\ezpm.exe", "win32"),
			'"C:\\Program Files\\ezpm\\ezpm.exe"',
		);
	});
});
