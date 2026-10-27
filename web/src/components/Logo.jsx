export default function Logo({ size = 26 }) {
  return (
    <span className="logo">
      <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
        <circle cx="16" cy="16" r="11" fill="none" stroke="var(--accent)" strokeWidth="3.5" />
        <circle cx="16" cy="16" r="3.4" fill="var(--accent)" />
      </svg>
      <span className="logo__text">
        <span className="logo__pi">Pi</span>CCTV
      </span>
    </span>
  )
}
