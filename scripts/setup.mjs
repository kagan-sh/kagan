import { existsSync } from "node:fs"
import { execSync } from "node:child_process"

if (existsSync(".git")) execSync("git config core.hooksPath .githooks", { stdio: "inherit" })
