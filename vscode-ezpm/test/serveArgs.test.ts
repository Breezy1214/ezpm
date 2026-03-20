import { strict as assert } from "node:assert";
import { buildServeArgs } from "../src/core/serveArgs";

describe("serve args", () => {
	it("does not add port when unset", () => {
		assert.deepEqual(buildServeArgs(), ["serve"]);
		assert.deepEqual(buildServeArgs(0), ["serve"]);
	});

	it("adds short -p flag when port is set", () => {
		assert.deepEqual(buildServeArgs(34872), ["serve", "-p", "34872"]);
	});
});
