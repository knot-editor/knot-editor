import { testLanguagePackage } from "knot/testing/languages";
import { start } from "../src/extension.js";
import { FILE_ICONS, LANGUAGES } from "../src/languages.js";

testLanguagePackage(start, LANGUAGES, FILE_ICONS, ["python"]);
