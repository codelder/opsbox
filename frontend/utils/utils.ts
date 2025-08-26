export function path_match(path: string, pattern: string): boolean {
  path = path.replace(/^\/|\/$/g, "");
  pattern = pattern.replace(/^\/|\/$/g, "");

  const pathParts = path.split("/");
  const patternParts = pattern.split("/");

  if (pathParts.length < patternParts.length) {
    return false;
  }

  for (let i = 0; i < patternParts.length; i++) {
    const pathPart = pathParts[i];
    const patternPart = patternParts[i];

    if (patternPart === "*") {
      return true;
    }

    if (pathPart !== patternPart) {
      return false;
    }
  }

  return true;
}
