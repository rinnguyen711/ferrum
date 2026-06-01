import { Icons } from "../components/icons";

const TOKENS = [
  { name: "Production read-only", type: "Read-only", last: "11m ago", key: "rst_live_a91f…c4e2" },
  { name: "Website ISR", type: "Custom", last: "2h ago", key: "rst_live_77b0…91da" },
  { name: "Local dev", type: "Full access", last: "3d ago", key: "rst_test_0c3e…ab19" },
];

export function Settings() {
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>API tokens</h1>
          <p className="rs-cm-sub">
            Tokens authenticate requests to the Rust API. Treat them like passwords.
          </p>
        </div>
        <button className="rs-btn rs-btn--primary">
          <Icons.plus size={16} /> Create new token
        </button>
      </div>
      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Token</th>
              <th>Last used</th>
              <th className="rs-col-act" />
            </tr>
          </thead>
          <tbody>
            {TOKENS.map((t) => (
              <tr key={t.name}>
                <td className="rs-cell-title">
                  <span className="rs-title-text">{t.name}</span>
                </td>
                <td>
                  <span className="rs-type-pill">{t.type}</span>
                </td>
                <td className="rs-mono rs-cell-muted">{t.key}</td>
                <td className="rs-cell-muted">{t.last}</td>
                <td className="rs-col-act">
                  <button className="rs-row-btn">
                    <Icons.copy size={16} />
                  </button>
                  <button className="rs-row-btn rs-danger">
                    <Icons.trash size={16} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
