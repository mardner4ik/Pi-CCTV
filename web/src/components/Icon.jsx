const PATHS = {
  lock: 'M5 11h14v9H5zM8 11V8a4 4 0 018 0v3',
  logout: 'M10 4H5v16h5M15 8l4 4-4 4M19 12H9',
  plus: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6L6 18',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  trash: 'M5 7h14M10 11v6M14 11v6M7 7l1 13h8l1-13M9 7V4h6v3',
  edit: 'M4 20h4L19 9l-4-4L4 16z',
  refresh: 'M20 5v5h-5M4 19v-5h5M19 10a7 7 0 00-12-3L4 10M5 14a7 7 0 0012 3l3-3',
  power: 'M12 3v9M6.3 6.8a8 8 0 1011.4 0',
  search: 'M11 4a7 7 0 100 14 7 7 0 000-14zM16 16l5 5',
  camera: 'M3 7h13v10H3zM16 10l5-3v10l-5-3',
  wifi: 'M2 9a15 15 0 0120 0M5 12.5a10 10 0 0114 0M8.5 16a5 5 0 017 0M12 19.5h.01',
  globe: 'M12 3a9 9 0 100 18 9 9 0 000-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18',
  usb: 'M12 20V4M12 4l-2.5 3M12 4l2.5 3M12 13l5-3V8M12 15l-5-3v-2',
  snapshot: 'M4 8h3l2-3h6l2 3h3v11H4zM12 11a3 3 0 100 6 3 3 0 000-6z',
  expand: 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5',
  enlarge: 'M4 4h16v16H4z',
  shrink: 'M4 4h16v16H4zM9 9h6v6H9z',
  eye: 'M3 12s3.5-7 9-7 9 7 9 7-3.5 7-9 7-9-7-9-7zM12 9a3 3 0 100 6 3 3 0 000-6z',
  eyeOff: 'M3 3l18 18M10.6 6.2A9 9 0 0121 12a13 13 0 01-3 3.7M6.4 6.5A13 13 0 003 12s3.5 7 9 7a9 9 0 003.5-.7M9.9 9.9a3 3 0 004.2 4.2',
  chip: 'M6 6h12v12H6zM9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4',
  play: 'M7 4l13 8-13 8z',
}

export default function Icon({ name, size = 18 }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name] || ''} />
    </svg>
  )
}
