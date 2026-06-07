export function SelectBox({ checked }: { checked: boolean }) {
  return (
    <span className={"rs-media-selbox" + (checked ? " is-checked" : "")} aria-hidden="true">
      {checked && (
        <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="1.5,5.5 4.5,8.5 9.5,2.5" />
        </svg>
      )}
    </span>
  );
}
