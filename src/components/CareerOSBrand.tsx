import { useId } from "react";

export function CareerOSMark({ className = "" }: { className?: string }) {
  const prefix = useId().replaceAll(":", "");
  const ring = `${prefix}-ring`;
  const planeTop = `${prefix}-plane-top`;
  const planeFold = `${prefix}-plane-fold`;
  const openC = `${prefix}-open-c`;
  return <svg className={`careeros-mark ${className}`.trim()} viewBox="0 0 1024 1024" aria-hidden="true">
    <defs>
      <linearGradient id={ring} x1="180" y1="130" x2="760" y2="870" gradientUnits="userSpaceOnUse">
        <stop stopColor="var(--mark-ring-start, #74dfd9)" /><stop offset="0.52" stopColor="var(--mark-ring-mid, #2ab6c0)" /><stop offset="1" stopColor="var(--mark-ring-end, #0b8296)" />
      </linearGradient>
      <linearGradient id={planeTop} x1="330" y1="520" x2="825" y2="300" gradientUnits="userSpaceOnUse">
        <stop stopColor="var(--mark-plane-start, #f4fbf9)" /><stop offset="1" stopColor="var(--mark-plane-end, #a9ece6)" />
      </linearGradient>
      <linearGradient id={planeFold} x1="560" y1="560" x2="710" y2="720" gradientUnits="userSpaceOnUse">
        <stop stopColor="#2bb7bf" /><stop offset="1" stopColor="#087287" />
      </linearGradient>
      <mask id={openC} maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">
        <rect width="1024" height="1024" fill="white" /><path d="M512 512 986 212v600Z" fill="black" />
      </mask>
    </defs>
    <circle cx="512" cy="512" r="343" fill="none" stroke={`url(#${ring})`} strokeWidth="150" mask={`url(#${openC})`} />
    <path d="m320 514 500-174-260 225Z" fill={`url(#${planeTop})`} />
    <path d="m560 565 260-225-200 370Z" fill={`url(#${planeFold})`} />
  </svg>;
}
