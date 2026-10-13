// 把 vite 构建产物内联成零依赖单文件 dist/preview.html。
// 用途：界面原型需要在没有服务、没有 Node 环境的情况下也能双击打开。
// 用法：node scripts/build-preview.mjs（前置：已执行 vite build --base ./）

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const html = readFileSync(join(dist, "index.html"), "utf8");

const jsName = html.match(/src="\.\/assets\/([^"]+\.js)"/)?.[1];
const cssName = html.match(/href="\.\/assets\/([^"]+\.css)"/)?.[1];
if (!jsName || !cssName) {
  throw new Error("dist/index.html 里找不到 ./assets 引用，请先用 --base ./ 构建");
}

const js = readFileSync(join(dist, "assets", jsName), "utf8").replaceAll("</script", "<\\/script");
const css = readFileSync(join(dist, "assets", cssName), "utf8").replaceAll("</style", "<\\/style");

// 两个必须注意的点：
// 1) 必须用函数式替换。字符串形式的替换会把 JS 里的 $& / $` / $' 当成替换模式，
//    导致内联内容被 HTML 片段污染、产出语法错误的文件。
// 2) 内联脚本必须放在 </body> 之前。经典脚本不会像 module 那样自动 defer，
//    放在 <head> 里会早于 #app 解析执行，Vue 挂载目标不存在且不会报错。
//    这里也不用 type="module"，因为 file:// 下 module 脚本受 CORS 限制无法加载外部 chunk。
const out = html
  .replace(/<script type="module"[^>]*><\/script>\s*/, "")
  .replace(/<link rel="stylesheet"[^>]*>/, () => `<style>\n${css}\n</style>`)
  .replace(/<\/body>/, () => `<script>\n${js}\n</script>\n</body>`);

if (out.includes("assets/index-")) throw new Error("仍有外部资源引用，内联未完成");
if (!out.includes("<script>")) throw new Error("脚本未被内联");

writeFileSync(join(dist, "preview.html"), out);
console.log(`dist/preview.html 已生成（${out.length} 字节）`);
