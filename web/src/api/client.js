let unauthorizedHandler = () => {}

export function onUnauthorized(fn) {
  unauthorizedHandler = fn
}

export async function request(method, url, body, options = {}) {
  const init = { method, headers: {}, cache: 'no-store' }
  if (body !== undefined) {
    init.headers['Content-Type'] = 'application/json'
    init.body = JSON.stringify(body)
  }
  let timer
  if (options.timeout) {
    const ctl = new AbortController()
    timer = setTimeout(() => ctl.abort(), options.timeout)
    init.signal = ctl.signal
  }
  let res
  try {
    res = await fetch(url, init)
  } catch {
    return { ok: false, status: 0, data: null }
  } finally {
    clearTimeout(timer)
  }
  let data = null
  if (res.status !== 204) {
    try {
      data = await res.json()
    } catch {
      data = null
    }
  }
  if (res.status === 401 && !url.startsWith('/api/login')) unauthorizedHandler()
  return { ok: res.ok, status: res.status, data }
}

export const api = {
  get: (url, options) => request('GET', url, undefined, options),
  post: (url, body) => request('POST', url, body),
  put: (url, body) => request('PUT', url, body),
  del: (url) => request('DELETE', url),
}
