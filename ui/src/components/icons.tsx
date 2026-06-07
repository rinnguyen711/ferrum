import type { CSSProperties, ReactNode, SVGProps } from "react";

interface IcProps extends Omit<SVGProps<SVGSVGElement>, "d"> {
  d: string | ReactNode;
  size?: number;
  fill?: string;
  style?: CSSProperties;
  className?: string;
}

const Ic = ({ d, size = 18, fill, ...p }: IcProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill={fill || "none"}
    stroke="currentColor"
    strokeWidth={1.6}
    strokeLinecap="round"
    strokeLinejoin="round"
    {...p}
  >
    {typeof d === "string" ? <path d={d} /> : d}
  </svg>
);

export type IconProps = Omit<SVGProps<SVGSVGElement>, "d"> & { size?: number };

export const Icons = {
  grid: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></g>} />
  ),
  doc: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M6 2.5h7l5 5V21a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z"/><path d="M13 2.5V8h5"/></g>} />
  ),
  layers: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M12 3 3 8l9 5 9-5-9-5Z"/><path d="m3 13 9 5 9-5"/></g>} />
  ),
  image: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8.5" cy="9.5" r="1.5"/><path d="m4 18 5-5 4 3 3-2 4 4"/></g>} />
  ),
  gear: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="12" cy="12" r="3"/><path d="M19.4 12a7.4 7.4 0 0 0-.1-1.2l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2-1.2l-.3-2.5h-4l-.4 2.5a7 7 0 0 0-2 1.2l-2.3-1-2 3.4 2 1.5a7.4 7.4 0 0 0 0 2.4l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2 1.2l.4 2.5h4l.3-2.5a7 7 0 0 0 2-1.2l2.3 1 2-3.4-2-1.5c.1-.4.1-.8.1-1.2Z"/></g>} />
  ),
  user: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></g>} />
  ),
  tag: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M3 3h7l11 11-7 7L3 10V3Z"/><circle cx="7.5" cy="7.5" r="1.3"/></g>} />
  ),
  home: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M4 11 12 4l8 7"/><path d="M6 10v9a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1v-9"/></g>} />
  ),
  globe: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.5 2.5 15 0 18M12 3c-2.5 2.5-2.5 15 0 18"/></g>} />
  ),
  plus: (p: IconProps) => <Ic {...p} d="M12 5v14M5 12h14" />,
  search: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></g>} />
  ),
  filter: (p: IconProps) => <Ic {...p} d="M3 5h18l-7 8v6l-4-2v-4L3 5Z" />,
  chevDown: (p: IconProps) => <Ic {...p} d="m6 9 6 6 6-6" />,
  chevRight: (p: IconProps) => <Ic {...p} d="m9 6 6 6-6 6" />,
  chevLeft: (p: IconProps) => <Ic {...p} d="m15 6-6 6 6 6" />,
  dots: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none"/></g>} />
  ),
  trash: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"/></g>} />
  ),
  edit: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M4 20h4L19 9l-4-4L4 16v4Z"/><path d="m14 6 4 4"/></g>} />
  ),
  eye: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z"/><circle cx="12" cy="12" r="3"/></g>} />
  ),
  check: (p: IconProps) => <Ic {...p} d="M5 12.5 10 17l9-10" />,
  x: (p: IconProps) => <Ic {...p} d="M6 6l12 12M18 6 6 18" />,
  bolt: (p: IconProps) => <Ic {...p} d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" />,
  bell: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M6 9a6 6 0 0 1 12 0c0 5 2 6 2 6H4s2-1 2-6Z"/><path d="M10 20a2 2 0 0 0 4 0"/></g>} />
  ),
  copy: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h8"/></g>} />
  ),
  drag: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="9" cy="6" r="1.3" fill="currentColor" stroke="none"/><circle cx="9" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="9" cy="18" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="6" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="18" r="1.3" fill="currentColor" stroke="none"/></g>} />
  ),
  link: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M10 14a4 4 0 0 0 5.6 0l3-3a4 4 0 0 0-5.6-5.6L11.5 7"/><path d="M14 10a4 4 0 0 0-5.6 0l-3 3a4 4 0 0 0 5.6 5.6L12.5 17"/></g>} />
  ),
  type: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M4 7V5h16v2M9 19h6M12 5v14"/></g>} />
  ),
  hash: (p: IconProps) => <Ic {...p} d="M5 9h14M5 15h14M10 4 8 20M16 4l-2 16" />,
  calendar: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 9h18M8 3v4M16 3v4"/></g>} />
  ),
  toggle: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="2" y="7" width="20" height="10" rx="5"/><circle cx="16" cy="12" r="3" fill="currentColor" stroke="none"/></g>} />
  ),
  relation: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="6" cy="6" r="3"/><circle cx="18" cy="18" r="3"/><path d="M9 6h5a2 2 0 0 1 2 2v7"/></g>} />
  ),
  sort: (p: IconProps) => (
    <Ic {...p} d="M8 5v14m0 0-3-3m3 3 3-3M16 19V5m0 0-3 3m3-3 3 3" />
  ),
  external: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M14 4h6v6M20 4l-9 9"/><path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/></g>} />
  ),
  clock: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></g>} />
  ),
  lock: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/></g>} />
  ),
  star: (p: IconProps) => (
    <Ic {...p} d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8L3.5 9.7l5.9-.9L12 3.5Z" />
  ),
  arrowLeft: (p: IconProps) => <Ic {...p} d="M19 12H5m0 0 6-6m-6 6 6 6" />,
  sun: (p: IconProps) => (
    <Ic {...p} d={<g><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></g>} />
  ),
  moon: (p: IconProps) => (
    <Ic {...p} d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z" />
  ),
  folder: (p: IconProps) => (
    <Ic {...p} d={<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/>} />
  ),
  folderPlus: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/><path d="M12 11v4M10 13h4"/></g>} />
  ),
  folderInput: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/><path d="M12 11v5M9.5 13.5 12 16l2.5-2.5"/></g>} />
  ),
  upload: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M12 16V4M8 8l4-4 4 4"/><path d="M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/></g>} />
  ),
  mail: (p: IconProps) => (
    <Ic {...p} d={<g><rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/></g>} />
  ),
  braces: (p: IconProps) => (
    <Ic {...p} d={<g><path d="M8 4a3 3 0 0 0-3 3v2a2 2 0 0 1-2 2 2 2 0 0 1 2 2v2a3 3 0 0 0 3 3"/><path d="M16 4a3 3 0 0 1 3 3v2a2 2 0 0 0 2 2 2 2 0 0 0-2 2v2a3 3 0 0 1-3 3"/></g>} />
  ),
} as const;

export type IconKey = keyof typeof Icons;
