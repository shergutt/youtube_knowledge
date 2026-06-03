export function slugify(input: string, maxLen = 60): string {
  let out = "";
  let lastDash = true;
  for (const ch of input) {
    if (/[a-zA-Z0-9]/.test(ch)) {
      if (out.length >= maxLen) break;
      out += ch.toLowerCase();
      lastDash = false;
    } else if (!lastDash) {
      if (out.length >= maxLen) break;
      out += "-";
      lastDash = true;
    }
  }
  const trimmed = out.replace(/-+$/, "");
  return trimmed || "untitled";
}

export function themedBase(title: string, videoId: string): string {
  const slug = slugify(title);
  const shortId = videoId.slice(0, 6);
  return `${slug}-${shortId}`;
}
