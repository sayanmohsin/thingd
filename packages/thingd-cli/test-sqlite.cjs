const { execSync } = require('child_process');
try {
  const query = `
PRAGMA busy_timeout=2000;
SELECT count(*) FROM objects;
SELECT count(*) FROM events;
SELECT count(*) FROM queue_jobs WHERE status != 'dead';
SELECT count(*) FROM queue_jobs WHERE status = 'dead';
  `.trim();
  const output = execSync(`sqlite3 "/Users/sayanmohsin/Downloads/data.db" "${query}"`).toString().trim().split('\n');
  console.log("Success:", output);
} catch (e) {
  console.error("Failed:", e.message);
  if (e.stdout) console.error("Stdout:", e.stdout.toString());
  if (e.stderr) console.error("Stderr:", e.stderr.toString());
}
