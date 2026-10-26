export function uploadName(bundleName) {
  return bundleName.replaceAll(" ", ".");
}

export function downloadUrl(repo, version, name) {
  return `https://github.com/${repo}/releases/download/${version}/${name}`;
}

export function latestJson({ version, notes, signature, url, date }) {
  const entry = { signature: signature.trim(), url };
  return {
    version,
    notes,
    pub_date: date.toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: { "windows-x86_64": entry, "windows-x86_64-nsis": entry },
  };
}
