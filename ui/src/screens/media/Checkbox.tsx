import { Icons } from "../../components/icons";

export function Checkbox({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
      role="checkbox"
      aria-checked={checked}
      type="button"
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}
