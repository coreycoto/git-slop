export function createReleaseApi({ githubToken }) {
  function requestHeaders(accept = "application/vnd.github+json") {
    const headers = {
      Accept: accept,
      "User-Agent": "coreycoto-git-slop-action",
      "X-GitHub-Api-Version": "2022-11-28",
    };
    if (githubToken) {
      headers.Authorization = `Bearer ${githubToken}`;
    }
    return headers;
  }

  async function fetchRequired(url, accept) {
    const response = await fetch(url, {
      headers: requestHeaders(accept),
      redirect: "follow",
    });
    if (!response.ok) {
      throw new Error(`${url} returned HTTP ${response.status}`);
    }
    return response;
  }

  async function fetchPublicRequired(url, accept = "application/octet-stream") {
    const response = await fetch(url, {
      headers: {
        Accept: accept,
        "User-Agent": "coreycoto-git-slop-action",
      },
      redirect: "follow",
    });
    if (!response.ok) {
      throw new Error(`${url} returned HTTP ${response.status}`);
    }
    return response;
  }

  async function readResponseBounded(response, maximumBytes, label, expectedBytes = null) {
    const declaredLength = response.headers.get("content-length");
    if (declaredLength !== null) {
      if (!/^\d+$/u.test(declaredLength)) {
        throw new Error(`${label} returned an invalid Content-Length`);
      }
      const parsedLength = Number(declaredLength);
      if (!Number.isSafeInteger(parsedLength) || parsedLength > maximumBytes) {
        throw new Error(`${label} exceeds its download size limit`);
      }
      if (expectedBytes !== null && parsedLength !== expectedBytes) {
        throw new Error(
          `${label} Content-Length mismatch: expected ${expectedBytes}, received ${parsedLength}`,
        );
      }
    }
    if (!response.body) {
      throw new Error(`${label} returned no response body`);
    }
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      size += value.byteLength;
      if (size > maximumBytes || (expectedBytes !== null && size > expectedBytes)) {
        await reader.cancel();
        throw new Error(`${label} exceeds its download size limit`);
      }
      chunks.push(Buffer.from(value));
    }
    if (expectedBytes !== null && size !== expectedBytes) {
      throw new Error(`${label} size mismatch: expected ${expectedBytes}, received ${size}`);
    }
    return Buffer.concat(chunks, size);
  }

  async function fetchJsonRequired(url, maximumBytes, label) {
    const response = await fetchRequired(url);
    const bytes = await readResponseBounded(response, maximumBytes, label);
    try {
      return JSON.parse(bytes.toString("utf8"));
    } catch (error) {
      throw new Error(`${label} returned invalid JSON: ${error.message}`);
    }
  }

  async function downloadAsset(asset, maximumBytes) {
    const response = await fetchRequired(asset.url, "application/octet-stream");
    return readResponseBounded(response, maximumBytes, `release asset ${asset.name}`, asset.size);
  }

  return {
    downloadAsset,
    fetchJsonRequired,
    fetchPublicRequired,
    readResponseBounded,
  };
}
