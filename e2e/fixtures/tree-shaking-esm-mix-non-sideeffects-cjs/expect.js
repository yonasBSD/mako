const assert = require("assert");
const { parseBuildResult, moduleReg } = require("../../../scripts/test-utils");
const { files } = parseBuildResult(__dirname);

const names = Object.keys(files).join(",");
const content = files["index.js"];

assert(!content.includes(`cjs`), "should not have cjs.js module define");