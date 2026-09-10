import { cp, mkdir, rm } from "node:fs/promises";

await rm("dist", { recursive: true, force: true });
await mkdir("dist", { recursive: true });
await cp("Hiload Plant Latest.html", "dist/Hiload Plant Latest.html");

console.log("Prepared dist/Hiload Plant Latest.html for the Windows installer.");
