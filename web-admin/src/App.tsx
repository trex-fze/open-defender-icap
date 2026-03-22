import { useState } from 'react';

function App() {
  const [status] = useState('Open Defender ICAP Admin');

  return (
    <main style={{ fontFamily: 'IBM Plex Sans, system-ui', padding: '2rem', background: 'linear-gradient(120deg,#001724,#003f5c)' }}>
      <section style={{ background: 'rgba(255,255,255,0.08)', padding: '2rem', borderRadius: '1rem', color: '#f2f4f7' }}>
        <h1 style={{ marginTop: 0 }}>{status}</h1>
        <p>React dashboard placeholder. Implement dashboards, investigations, policy workflows per specification.</p>
      </section>
    </main>
  );
}

export default App;
