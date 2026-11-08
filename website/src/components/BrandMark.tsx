type BrandMarkProps = {
  className?: string;
};

export function BrandMark({ className }: BrandMarkProps) {
  return (
    <span className={className} aria-hidden="true">
      <svg width="30" height="30" viewBox="0 0 30 30" fill="none">
        <rect x="2" y="11" width="3" height="8" rx="1.5" fill="currentColor" opacity="0.7" />
        <rect x="8" y="7" width="3" height="16" rx="1.5" fill="currentColor" />
        <rect x="14" y="2" width="3" height="26" rx="1.5" fill="#ea7a4c" />
        <rect x="20" y="9" width="3" height="12" rx="1.5" fill="currentColor" />
        <rect x="26" y="12" width="2.5" height="6" rx="1.25" fill="currentColor" opacity="0.7" />
      </svg>
    </span>
  );
}
