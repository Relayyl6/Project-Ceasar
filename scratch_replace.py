import re
import os

agri_raw = r"""1: <!DOCTYPE html>
2: <html lang="en">
3: <head>
4:   <meta charset="utf-8"/>
5:   <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
6:   <title>URIEL MONOLITH — Agri 4.0</title>
7:   <link href="https://fonts.googleapis.com/css2?family=Share+Tech+Mono&family=Rajdhani:wght@400;500;600;700&family=Orbitron:wght@400;700;900&display=swap" rel="stylesheet"/>
8:   <link href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:wght,FILL@100..700,0..1&display=swap" rel="stylesheet"/>
9:   <style>
10:     :root {
11:       --bg: #060b0e;
12:       --bg2: #09101a;
13:       --bg3: #0c1620;
14:       --surface: #111d26;
15:       --surface2: #1a2d3a;
16:       --cyan: #00d4c8;
17:       --cyan-dim: rgba(0,212,200,0.1);
18:       --green: #00ff88;
19:       --green-dim: rgba(0,255,136,0.1);
20:       --amber: #ffcc00;
21:       --amber-dim: rgba(255,204,0,0.1);
22:       --red: #ff4455;
23:       --text: #b8d4cc;
24:       --text-dim: #4a6a60;
25:       --border: rgba(0,212,200,0.12);
26:       --border-s: rgba(0,212,200,0.28);
27:     }
28:     *{margin:0;padding:0;box-sizing:border-box;}
29:     html,body{height:100%;overflow:hidden;background:var(--bg);color:var(--text);font-family:'Rajdhani',sans-serif;}
30: 
31:     /* HEADER */
32:     header{
33:       position:fixed;top:0;left:0;right:0;z-index:100;height:48px;
34:       display:flex;align-items:center;justify-content:space-between;
35:       background:rgba(6,11,14,0.96);border-bottom:1px solid var(--border-s);
36:       padding:0 24px;backdrop-filter:blur(10px);
37:     }
38:     .brand{font-family:'Orbitron',monospace;font-size:1rem;font-weight:900;color:var(--cyan);letter-spacing:.2em;}
39:     .header-nav{display:flex;gap:0;}
40:     .nav-tab{
41:       padding:0 18px;height:48px;line-height:48px;
42:       font-family:'Share Tech Mono',monospace;font-size:.68rem;letter-spacing:.12em;text-transform:uppercase;
43:       color:var(--text-dim);cursor:pointer;border-bottom:2px solid transparent;transition:.15s;
44:     }
45:     .nav-tab.active{color:var(--cyan);border-bottom-color:var(--cyan);}
46:     .nav-tab:hover{color:var(--cyan);}
47:     .header-right{display:flex;align-items:center;gap:16px;}
48:     .header-search{
49:       display:flex;align-items:center;gap:8px;
50:       background:var(--surface);border:1px solid var(--border);padding:6px 14px;
51:       font-family:'Share Tech Mono',monospace;font-size:.65rem;color:var(--text-dim);
52:     }
53:     .icon-btn{width:36px;height:36px;display:flex;align-items:center;justify-content:center;color:var(--text-dim);cursor:pointer;transition:.15s;}
54:     .icon-btn:hover{color:var(--cyan);}
55:     .material-symbols-outlined{font-size:20px;font-variation-settings:'FILL' 0,'wght' 400;}
56: 
57:     /* SIDEBAR */
58:     aside{
59:       position:fixed;left:0;top:48px;bottom:0;width:200px;z-index:50;
60:       background:var(--bg2);border-right:1px solid var(--border);
61:       display:flex;flex-direction:column;
62:     }
63:     .sidebar-header{padding:14px 16px;}
64:     .strat-com-label{display:flex;align-items:center;gap:8px;margin-bottom:4px;}
65:     .strat-com-bar{width:3px;height:28px;background:var(--cyan);}
66:     .strat-com-text{font-family:'Orbitron',monospace;font-size:.75rem;font-weight:700;color:var(--cyan);}
67:     .strat-com-sub{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);letter-spacing:.15em;}
68:     .sidebar-divider{height:1px;background:var(--border);margin:8px 0;}
69:     .sidebar-item{
70:       display:flex;align-items:center;gap:10px;padding:11px 16px;
71:       font-size:.8rem;font-weight:600;letter-spacing:.04em;text-transform:uppercase;
72:       color:var(--text-dim);cursor:pointer;transition:.15s;text-decoration:none;
73:       border-left:3px solid transparent;
74:     }
75:     .sidebar-item:hover{color:var(--cyan);background:var(--cyan-dim);}
76:     .sidebar-item.active{color:var(--cyan);background:var(--cyan-dim);border-left-color:var(--cyan);}
77:     .sidebar-item .material-symbols-outlined{font-size:17px;}
78: 
79:     /* MAIN */
80:     main{
81:       position:fixed;left:200px;top:48px;right:0;bottom:0;overflow-y:auto;background:var(--bg);
82:     }
83: 
84:     /* HERO SECTION */
85:     .hero{
86:       padding:20px 24px 16px;border-bottom:1px solid var(--border);
87:       display:grid;grid-template-columns:1fr auto;gap:24px;align-items:start;
88:     }
89:     .sector-tag{
90:       display:inline-flex;align-items:center;gap:6px;
91:       background:var(--cyan-dim);border:1px solid var(--border-s);
92:       padding:3px 10px;font-family:'Share Tech Mono',monospace;font-size:.62rem;
93:       letter-spacing:.12em;text-transform:uppercase;color:var(--cyan);margin-bottom:8px;
94:     }
95:     .hero-title{font-family:'Orbitron',monospace;font-size:2.2rem;font-weight:900;color:#e8f4f0;letter-spacing:.05em;line-height:1;}
96:     .hero-desc{font-size:.9rem;color:var(--text);margin-top:8px;line-height:1.5;max-width:480px;}
97:     .hero-stats{display:grid;grid-template-columns:1fr 1fr;gap:16px;}
98:     .hero-stat-card{
99:       border:1px solid var(--border);padding:12px 16px;background:var(--surface);
100:     }
101:     .hero-stat-label{font-family:'Share Tech Mono',monospace;font-size:.6rem;letter-spacing:.12em;text-transform:uppercase;color:var(--text-dim);}
102:     .hero-stat-value{font-family:'Orbitron',monospace;font-size:1.6rem;font-weight:700;color:var(--green);margin:4px 0 2px;}
103:     .hero-stat-status{font-family:'Share Tech Mono',monospace;font-size:.62rem;color:var(--green);}
104:     .hero-stat-value.cyan{color:var(--cyan);}
105:     .hero-stat-status.cyan{color:var(--cyan);}
106: 
107:     /* MAIN GRID */
108:     .content-grid{display:grid;grid-template-columns:1fr 280px;height:calc(100vh - 48px - 120px);}
109: 
110:     /* YIELD MAP */
111:     .yield-section{padding:16px;border-right:1px solid var(--border);}
112:     .section-header{display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;}
113:     .section-title{font-family:'Share Tech Mono',monospace;font-size:.68rem;letter-spacing:.15em;text-transform:uppercase;color:var(--text);}
114:     .section-sub{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);}
115:     .tag{padding:3px 8px;font-family:'Share Tech Mono',monospace;font-size:.58rem;letter-spacing:.1em;text-transform:uppercase;border:1px solid;}
116:     .tag-cyan{color:var(--cyan);border-color:var(--border-s);}
117:     .tag-green{color:var(--green);border-color:rgba(0,255,136,.3);}
118:     .yield-canvas-wrap{position:relative;background:var(--surface);height:350px;overflow:hidden;}
119:     #yieldCanvas{width:100%;height:100%;}
120:     .yield-anomaly-tag{
121:       position:absolute;top:50%;left:50%;transform:translate(-30px,-20px);
122:       background:rgba(6,11,14,.9);border:1px solid rgba(255,68,85,.5);
123:       padding:4px 10px;font-family:'Share Tech Mono',monospace;font-size:.6rem;
124:       color:var(--red);letter-spacing:.08em;
125:     }
126:     .yield-metric{
127:       position:absolute;bottom:16px;left:16px;
128:       background:rgba(6,11,14,.88);border:1px solid var(--border-s);padding:10px 14px;
129:     }
130:     .yield-metric-label{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);text-transform:uppercase;}
131:     .yield-metric-value{font-family:'Orbitron',monospace;font-size:1.5rem;font-weight:700;color:var(--green);margin-top:2px;}
132:     .yield-bars{position:absolute;bottom:16px;right:16px;display:flex;flex-direction:column;gap:3px;}
133:     .ybar{height:5px;background:var(--cyan);opacity:.8;}
134: 
135:     /* RIGHT TELEMETRY */
136:     .telemetry-panel{padding:16px;border-left:1px solid var(--border);display:flex;flex-direction:column;gap:0;}
137:     .telem-card{padding:14px 0;border-bottom:1px solid var(--border);}
138:     .telem-card:last-child{border-bottom:none;}
139:     .telem-header{display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;}
140:     .telem-icon{width:28px;height:28px;border-radius:50%;border:1px solid var(--border-s);display:flex;align-items:center;justify-content:center;color:var(--cyan);}
141:     .telem-source{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);text-transform:uppercase;letter-spacing:.1em;}
142:     .telem-label{font-family:'Share Tech Mono',monospace;font-size:.68rem;letter-spacing:.12em;text-transform:uppercase;color:var(--text-dim);margin-bottom:4px;}
143:     .telem-value{font-family:'Orbitron',monospace;font-size:2rem;font-weight:700;color:var(--green);}
144:     .telem-unit{font-size:.9rem;font-weight:400;}
145:     .telem-sub{font-family:'Share Tech Mono',monospace;font-size:.6rem;color:var(--text-dim);margin-top:4px;}
146:     .telem-bar{height:3px;background:var(--surface2);margin-top:8px;}
147:     .telem-bar-fill{height:100%;background:var(--cyan);transition:width .8s;}
148:     .telem-extra{display:flex;justify-content:space-between;font-family:'Share Tech Mono',monospace;font-size:.6rem;color:var(--text-dim);margin-top:4px;}
149: 
150:     /* BOTTOM GRID */
151:     .bottom-grid{display:grid;grid-template-columns:1fr 1fr 1fr;border-top:1px solid var(--border);}
152: 
153:     /* AST CONTROL */
154:     .ast-panel,.fed-panel,.swarm-panel{padding:16px;border-right:1px solid var(--border);}
155:     .swarm-panel{border-right:none;}
156:     .panel-label{
157:       display:flex;align-items:center;gap:8px;font-family:'Share Tech Mono',monospace;
158:       font-size:.65rem;letter-spacing:.15em;text-transform:uppercase;color:var(--text-dim);margin-bottom:12px;
159:     }
160:     .ast-zone-card{background:var(--surface);border:1px solid var(--border);padding:10px 12px;margin-bottom:8px;display:flex;align-items:center;justify-content:space-between;}
161:     .ast-zone-label{font-family:'Share Tech Mono',monospace;font-size:.6rem;color:var(--text-dim);text-transform:uppercase;}
162:     .ast-zone-value{font-size:.9rem;font-weight:700;color:#e8f4f0;}
163:     .ast-zone-dot{width:8px;height:8px;background:var(--green);box-shadow:0 0 6px var(--green);}
164:     .ast-efficiency{display:flex;justify-content:space-between;font-family:'Share Tech Mono',monospace;font-size:.62rem;margin:10px 0 4px;}
165:     .eff-bar{height:4px;background:var(--surface2);}
166:     .eff-fill{height:100%;background:var(--cyan);transition:width .8s;}
167: 
168:     /* FED LEARNING */
169:     .fed-log{font-family:'Share Tech Mono',monospace;font-size:.62rem;line-height:1.8;}
170:     .fed-log-line{color:var(--text-dim);}
171:     .fed-log-line.active{color:var(--green);}
172:     .fed-log-line.label{color:var(--cyan);}
173:     .fed-metrics{display:flex;justify-content:space-between;margin-top:12px;gap:12px;}
174:     .fed-metric-box{flex:1;background:var(--surface);border:1px solid var(--border);padding:8px 10px;}
175:     .fed-metric-label{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);text-transform:uppercase;}
176:     .fed-metric-value{font-family:'Orbitron',monospace;font-size:1.2rem;font-weight:700;color:var(--green);}
177: 
178:     /* VTOL SWARM */
179:     .vtol-header{display:flex;align-items:start;justify-content:space-between;margin-bottom:12px;}
180:     .vtol-title{font-family:'Orbitron',monospace;font-size:.9rem;font-weight:700;color:#e8f4f0;}
181:     .vtol-status{font-family:'Share Tech Mono',monospace;font-size:.62rem;color:var(--green);background:var(--green-dim);border:1px solid rgba(0,255,136,.3);padding:2px 8px;}
182:     .vtol-images{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin-bottom:12px;}
183:     .vtol-img{height:80px;background:linear-gradient(135deg,#0c1a10,#0a1820);border:1px solid var(--border);display:flex;align-items:center;justify-content:center;position:relative;overflow:hidden;}
184:     .vtol-img-label{position:absolute;bottom:4px;left:4px;font-family:'Share Tech Mono',monospace;font-size:.55rem;color:var(--cyan);}
185:     .vtol-stat{display:flex;justify-content:space-between;align-items:center;padding:6px 0;border-bottom:1px solid var(--border);}
186:     .vtol-stat:last-child{border-bottom:none;}
187:     .vtol-stat-label{font-family:'Share Tech Mono',monospace;font-size:.62rem;color:var(--text-dim);text-transform:uppercase;}
188:     .vtol-stat-value{font-family:'Share Tech Mono',monospace;font-size:.68rem;color:var(--text);}
189:     .vtol-bar{width:80px;height:3px;background:var(--surface2);}
190:     .vtol-bar-fill{height:100%;background:var(--cyan);}
191: 
192:     /* FOOTER */
193:     footer{
194:       position:fixed;bottom:0;left:200px;right:0;height:26px;
195:       background:var(--bg2);border-top:1px solid var(--border);
196:       display:flex;align-items:center;padding:0 16px;gap:10px;
197:       font-family:'Share Tech Mono',monospace;font-size:.6rem;
198:     }
199:     .live-dot{width:6px;height:6px;background:var(--green);border-radius:50%;}
200:     .footer-status{color:var(--green);}
201:     .footer-div{color:var(--text-dim);}
202:     .footer-item{color:var(--text-dim);}
203:     .footer-enc{margin-left:auto;color:var(--text-dim);}
204: 
205:     /* SCROLLBAR */
206:     ::-webkit-scrollbar{width:3px;} ::-webkit-scrollbar-track{background:var(--bg);} ::-webkit-scrollbar-thumb{background:var(--surface2);}
207:     @keyframes pulse-dot{0%,100%{opacity:1}50%{opacity:.4}}
208:   </style>
209: </head>
210: <body>
211: 
212: <!-- HEADER -->
213: <header>
214:   <div class="brand">URIEL MONOLITH</div>
215:   <div class="header-nav">
216:     <div class="nav-tab">NODE HEALTH</div>
217:     <div class="nav-tab active">BWARI REGION</div>
218:     <div class="nav-tab">FCT SECTOR</div>
219:   </div>
220:   <div class="header-right">
221:     <div class="header-search">
222:       <span class="material-symbols-outlined" style="font-size:15px">search</span>
223:       QUERY SYSTEM…
224:     </div>
225:     <div class="icon-btn"><span class="material-symbols-outlined">sensors</span></div>
226:     <div class="icon-btn"><span class="material-symbols-outlined">hub</span></div>
227:     <div class="icon-btn"><span class="material-symbols-outlined">settings</span></div>
228:     <div class="icon-btn" style="width:32px;height:32px;border-radius:50%;background:var(--surface);border:1px solid var(--border-s);overflow:hidden;">
229:       <span class="material-symbols-outlined">person</span>
230:     </div>
231:   </div>
232: </header>
233: 
234: <!-- SIDEBAR -->
235: <aside>
236:   <div class="sidebar-header">
237:     <div class="strat-com-label">
238:       <div class="strat-com-bar"></div>
239:       <div>
240:         <div class="strat-com-text">STRAT-COM</div>
241:         <div class="strat-com-sub">V-01 S.W.A.R.M.</div>
242:       </div>
243:     </div>
244:   </div>
245:   <div class="sidebar-divider"></div>
246:   <a href="/" class="sidebar-item">
247:     <span class="material-symbols-outlined">shield</span> TROOP DEFENSE
248:   </a>
249:   <a href="/infra" class="sidebar-item">
250:     <span class="material-symbols-outlined">construction</span> INFRASTRUCTURE RESILIENCE
251:   </a>
252:   <a href="/agri" class="sidebar-item active">
253:     <span class="material-symbols-outlined">agriculture</span> AGRI 4.0
254:   </a>
255: </aside>
256: 
257: <!-- MAIN -->
258: <main>
259:   <!-- HERO -->
260:   <div class="hero">
261:     <div>
262:       <div class="sector-tag">SECTOR: FCT-BWARI &nbsp; LAT: 9.2882° N | LONG: 7.3821° E</div>
263:       <div class="hero-title">PROJECT CAESAR</div>
264:       <div class="hero-desc">Multi-agent precision agriculture network. Real-time inference via Distributed Gaussian Processes and Federated LSTM convergence.</div>
265:     </div>
266:     <div class="hero-stats">
267:       <div class="hero-stat-card">
268:         <div class="hero-stat-label">System Integrity</div>
269:         <div class="hero-stat-value" id="heroIntegrity">99.82%</div>
270:         <div class="hero-stat-status" id="heroStatus">NOMINAL</div>
271:       </div>
272:       <div class="hero-stat-card">
273:         <div class="hero-stat-label">Swarm Sync</div>
274:         <div class="hero-stat-value cyan" id="heroSwarm">ACTIVE: 128</div>
275:         <div class="hero-stat-status cyan">UNITS</div>
276:       </div>
277:     </div>
278:   </div>
279: 
280:   <!-- CONTENT GRID -->
281:   <div class="content-grid">
282:     <!-- YIELD SURFACE -->
283:     <div class="yield-section">
284:       <div class="section-header">
285:         <div>
286:           <div class="section-title">YIELD PREDICTION SURFACE</div>
287:           <div class="section-sub">Distributed Gaussian Process Visualization [σ=0.15]</div>
288:         </div>
289:         <div style="display:flex;gap:6px;">
290:           <div class="tag tag-cyan">3D RENDER</div>
291:           <div class="tag tag-green">REAL-TIME</div>
292:         </div>
293:       </div>
294:       <div class="yield-canvas-wrap">
295:         <canvas id="yieldCanvas"></canvas>
296:         <div class="yield-anomaly-tag">BWARI-092: ANOMALY</div>
297:         <div class="yield-metric">
298:           <div class="yield-metric-label">MAX PROJECTED YIELD</div>
299:           <div class="yield-metric-value" id="maxYield">8.42 t/ha</div>
300:         </div>
301:         <div class="yield-bars" id="yieldBars"></div>
302:       </div>
303:     </div>
304: 
305:     <!-- TELEMETRY PANEL -->
306:     <div class="telemetry-panel">
307:       <div class="telem-card">
308:         <div class="telem-header">
309:           <div class="telem-icon"><span class="material-symbols-outlined" style="font-size:16px">water_drop</span></div>
310:           <div class="telem-source">TELEMETRY: NODE 44</div>
311:         </div>
312:         <div class="telem-label">SOIL MOISTURE</div>
313:         <div class="telem-value"><span id="soilMoisture">62.4</span><span class="telem-unit"> % VWC</span></div>
314:         <div class="telem-bar"><div class="telem-bar-fill" id="soilBar" style="width:62.4%"></div></div>
315:       </div>
316:       <div class="telem-card">
317:         <div class="telem-header">
318:           <div class="telem-icon"><span class="material-symbols-outlined" style="font-size:16px">satellite_alt</span></div>
319:           <div class="telem-source">SATELLITE FUSION</div>
320:         </div>
321:         <div class="telem-label">NDVI INDEX</div>
322:         <div class="telem-value" style="font-size:1.8rem;"><span id="ndviVal">0.82</span> <span style="font-size:.85rem;color:var(--green)">VIGOR</span></div>
323:         <div class="telem-extra"><span>OPTIMAL RANGE</span><span id="ndviOverlap">80% OVERLAP</span></div>
324:         <div class="telem-bar"><div class="telem-bar-fill" id="ndviBar" style="width:82%"></div></div>
325:       </div>
326:       <div class="telem-card">
327:         <div class="telem-header">
328:           <div class="telem-icon"><span class="material-symbols-outlined" style="font-size:16px">thermostat</span></div>
329:           <div class="telem-source">ATMOSPHERIC</div>
330:         </div>
331:         <div class="telem-label">HUMIDITY/TEMP</div>
332:         <div class="telem-value"><span id="humidVal">84.1</span><span class="telem-unit"> RH%</span></div>
333:         <div class="telem-sub">↑ <span id="humidDelta">2.4%</span> FROM PREVIOUS 1H EPOCH</div>
334:         <div class="telem-bar"><div class="telem-bar-fill" id="humidBar" style="width:84.1%;background:var(--amber)"></div></div>
335:       </div>
336:     </div>
337:   </div>
338: 
339:   <!-- BOTTOM GRID -->
340:   <div class="bottom-grid">
341:     <!-- AST CONTROL -->
342:     <div class="ast-panel">
343:       <div class="panel-label"><span class="material-symbols-outlined" style="font-size:15px">settings_input_antenna</span> AST CONTROL</div>
344:       <div class="ast-zone-card">
345:         <div>
346:           <div class="ast-zone-label">ACTIVE ZONE</div>
347:           <div class="ast-zone-value" id="astZone">QUADRANT-DELTA</div>
348:         </div>
349:         <div class="ast-zone-dot"></div>
350:       </div>
351:       <div class="ast-zone-card">
352:         <div>
353:           <div class="ast-zone-label">WATER SAVINGS</div>
354:           <div class="ast-zone-value" id="waterSavings">+12,400L / WK</div>
355:         </div>
356:         <span class="material-symbols-outlined" style="color:var(--green);font-size:18px">trending_up</span>
357:       </div>
358:       <div class="ast-efficiency">
359:         <span>AST EFFICIENCY</span>
360:         <span id="astEff">92%</span>
361:       </div>
362:       <div class="eff-bar"><div class="eff-fill" id="astEffBar" style="width:92%"></div></div>
363:     </div>
364: 
365:     <!-- FEDERATED LEARNING -->
366:     <div class="fed-panel">
367:       <div class="panel-label"><span class="material-symbols-outlined" style="font-size:15px">hub</span> FEDERATED LEARNING</div>
368:       <div class="fed-log" id="fedLog">
369:         <div class="fed-log-line active">&gt; INITIALIZING LOCAL GRADIENT…</div>
370:         <div class="fed-log-line label">&gt; MODEL: LSTM-CROP-STRESS-V4</div>
371:         <div class="fed-log-line">&gt; BATCH SIZE: 512</div>
372:         <div class="fed-log-line">&gt; EPOCH <span id="fedEpoch">42</span>: LOSS=<span id="fedLossVal">0.0021</span></div>
373:         <div class="fed-log-line active">&gt; CONVERGENCE ACHIEVED: <span id="fedConv">0.994</span></div>
374:         <div class="fed-log-line">&gt; DISTRIBUTING UPDATE TO EDGE NODES…</div>
375:         <div class="fed-log-line">&gt; SYNCING GLOBAL AGGREGATOR…</div>
376:       </div>
377:       <div class="fed-metrics">
378:         <div class="fed-metric-box">
379:           <div class="fed-metric-label">CONFIDENCE</div>
380:           <div class="fed-metric-value" id="fedConfidence">0.982</div>
381:         </div>
382:         <div class="fed-metric-box">
383:           <div class="fed-metric-label">NODES CONNECTED</div>
384:           <div class="fed-metric-value" id="fedNodes">312 / 312</div>
385:         </div>
386:       </div>
387:     </div>
388: 
389:     <!-- VTOL SWARM -->
390:     <div class="swarm-panel">
391:       <div class="vtol-header">
392:         <div>
393:           <span class="material-symbols-outlined" style="font-size:16px;color:var(--cyan);vertical-align:middle">flight</span>
394:           <span class="vtol-title"> ARCHER VTOL SWARM</span>
395:         </div>
396:         <div class="vtol-status">MISSION ACTIVE</div>
397:       </div>
398:       <div class="vtol-images">
399:         <div class="vtol-img">
400:           <svg width="100%" height="100%" viewBox="0 0 120 80" style="opacity:.7">
401:             <path d="M20 60 Q40 20 60 15 Q80 20 100 60" stroke="#00d4c8" fill="none" stroke-width=".8"/>
402:             <path d="M0 70 L120 70" stroke="#1a2d3a" stroke-width=".5"/>
403:             <circle cx="60" cy="15" r="3" fill="#00ff88" opacity=".8"/>
404:           </svg>
405:           <div class="vtol-img-label">VTOL-A1 / FIELD SCAN</div>
406:         </div>
407:         <div class="vtol-img">
408:           <svg width="100%" height="100%" viewBox="0 0 120 80" style="opacity:.7">
409:             <path d="M10 70 Q30 30 60 25 Q90 30 110 70" stroke="#00d4c8" fill="none" stroke-width=".8" stroke-dasharray="4 2"/>
410:             <line x1="0" y1="45" x2="120" y2="45" stroke="#1a2d3a" stroke-width=".5"/>
411:           </svg>
412:           <div class="vtol-img-label">VTOL-A2 / IRRIGATION</div>
413:         </div>
414:       </div>
415:       <div class="vtol-stat">
416:         <div class="vtol-stat-label">VTOL-A1 BATTERY</div>
417:         <div style="display:flex;align-items:center;gap:8px;">
418:           <div class="vtol-bar"><div class="vtol-bar-fill" id="vBat1" style="width:88%"></div></div>
419:           <div class="vtol-stat-value" id="vBat1Val">88%</div>
420:         </div>
421:       </div>
422:       <div class="vtol-stat">
423:         <div class="vtol-stat-label">VTOL-A2 SIGNAL</div>
424:         <div class="vtol-stat-value" id="vSig2">-42dBm</div>
425:       </div>
426:     </div>
427:   </div>
428: </main>
429: 
430: <!-- FOOTER -->
431: <footer>
432:   <div class="live-dot" style="animation:pulse-dot 1.4s ease-in-out infinite"></div>
433:   <span class="footer-status">SYSTEM NOMINAL</span>
434:   <div class="footer-div">|</div>
435:   <span class="footer-item">AGRI-LINK ACTIVE</span>
436:   <div class="footer-div">|</div>
437:   <span class="footer-item">DEFENSE-GRID: SECURE</span>
438:   <span class="footer-enc">ENC: AES-256-XTS</span>
439: </footer>
440: 
441: <style>
442: @keyframes pulse-dot{0%,100%{opacity:1}50%{opacity:.4}}
443: </style>
444: 
445: <script src="/static/ceasar-api.js"></script>
446: <script src="/static/agri.js"></script>
447: </body>
448: </html>
449: """
infra_raw = r"""1: <!DOCTYPE html>
2: <html lang="en">
3: <head>
4:   <meta charset="utf-8"/>
5:   <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
6:   <title>URIEL MONOLITH — Infrastructure Resilience</title>
7:   <link href="https://fonts.googleapis.com/css2?family=Share+Tech+Mono&family=Rajdhani:wght@400;500;600;700&family=Orbitron:wght@400;700;900&display=swap" rel="stylesheet"/>
8:   <link href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:wght,FILL@100..700,0..1&display=swap" rel="stylesheet"/>
9:   <style>
10:     :root {
11:       --bg: #070a0f;
12:       --bg2: #0a0e16;
13:       --surface: #0f1520;
14:       --surface2: #182030;
15:       --cyan: #00ccff;
16:       --cyan-dim: rgba(0,204,255,0.08);
17:       --green: #00ee88;
18:       --green-dim: rgba(0,238,136,0.1);
19:       --amber: #ffbb00;
20:       --amber-dim: rgba(255,187,0,0.1);
21:       --red: #ff3344;
22:       --red-dim: rgba(255,51,68,0.1);
23:       --text: #aac4d8;
24:       --text-dim: #3a5565;
25:       --border: rgba(0,204,255,0.1);
26:       --border-s: rgba(0,204,255,0.22);
27:     }
28:     *{margin:0;padding:0;box-sizing:border-box;}
29:     html,body{height:100%;overflow:hidden;background:var(--bg);color:var(--text);font-family:'Rajdhani',sans-serif;}
30: 
31:     /* HEADER */
32:     header{
33:       position:fixed;top:0;left:0;right:0;z-index:100;height:48px;
34:       display:flex;align-items:center;justify-content:space-between;
35:       background:rgba(7,10,15,0.96);border-bottom:1px solid var(--border-s);
36:       padding:0 24px;backdrop-filter:blur(10px);
37:     }
38:     .brand{font-family:'Orbitron',monospace;font-size:1rem;font-weight:900;color:var(--cyan);letter-spacing:.2em;}
39:     .header-nav{display:flex;gap:0;}
40:     .nav-tab{padding:0 18px;height:48px;line-height:48px;font-family:'Share Tech Mono',monospace;font-size:.68rem;letter-spacing:.12em;text-transform:uppercase;color:var(--text-dim);cursor:pointer;border-bottom:2px solid transparent;transition:.15s;}
41:     .nav-tab.active{color:var(--cyan);border-bottom-color:var(--cyan);}
42:     .nav-tab:hover{color:var(--cyan);}
43:     .header-right{display:flex;align-items:center;gap:12px;}
44:     .header-search{display:flex;align-items:center;gap:8px;background:var(--surface);border:1px solid var(--border);padding:6px 14px;font-family:'Share Tech Mono',monospace;font-size:.65rem;color:var(--text-dim);}
45:     .icon-btn{width:36px;height:36px;display:flex;align-items:center;justify-content:center;color:var(--text-dim);cursor:pointer;transition:.15s;}
46:     .icon-btn:hover{color:var(--cyan);}
47:     .material-symbols-outlined{font-size:20px;font-variation-settings:'FILL' 0,'wght' 400;}
48: 
49:     /* SIDEBAR */
50:     aside{position:fixed;left:0;top:48px;bottom:0;width:200px;z-index:50;background:var(--bg2);border-right:1px solid var(--border);display:flex;flex-direction:column;}
51:     .strat-com-label{display:flex;align-items:center;gap:8px;padding:14px 16px;}
52:     .strat-com-bar{width:3px;height:28px;background:var(--cyan);}
53:     .strat-com-text{font-family:'Orbitron',monospace;font-size:.75rem;font-weight:700;color:var(--cyan);}
54:     .strat-com-sub{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);letter-spacing:.15em;}
55:     .sidebar-divider{height:1px;background:var(--border);}
56:     .sidebar-item{display:flex;align-items:center;gap:10px;padding:11px 16px;font-size:.8rem;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--text-dim);cursor:pointer;transition:.15s;text-decoration:none;border-left:3px solid transparent;}
57:     .sidebar-item:hover{color:var(--cyan);background:var(--cyan-dim);}
58:     .sidebar-item.active{color:var(--cyan);background:var(--cyan-dim);border-left-color:var(--cyan);}
59:     .sidebar-item .material-symbols-outlined{font-size:17px;}
60:     .deploy-btn-wrap{margin-top:auto;padding:16px;}
61:     .deploy-btn{width:100%;padding:11px;background:var(--cyan);border:none;color:#020810;font-family:'Share Tech Mono',monospace;font-size:.72rem;letter-spacing:.18em;text-transform:uppercase;cursor:pointer;font-weight:700;transition:.15s;}
62:     .deploy-btn:hover{filter:brightness(1.15);}
63: 
64:     /* MAIN */
65:     main{position:fixed;left:200px;top:48px;right:0;bottom:0;overflow-y:auto;background:var(--bg);}
66: 
67:     /* HERO HEADER */
68:     .infra-hero{padding:16px 20px 12px;border-bottom:1px solid var(--border);}
69:     .infra-title{font-family:'Orbitron',monospace;font-size:1.6rem;font-weight:900;color:#d8eaf8;letter-spacing:.04em;}
70:     .infra-status-bar{display:flex;align-items:center;gap:16px;margin-top:8px;}
71:     .status-dot{width:8px;height:8px;border-radius:50%;}
72:     .status-item{display:flex;align-items:center;gap:6px;font-family:'Share Tech Mono',monospace;font-size:.62rem;text-transform:uppercase;}
73:     .epoch-display{margin-left:auto;font-family:'Orbitron',monospace;font-size:.75rem;color:var(--text-dim);}
74:     .epoch-value{font-family:'Orbitron',monospace;font-size:1.1rem;font-weight:700;color:var(--cyan);}
75: 
76:     /* MAIN GRID */
77:     .main-grid{display:grid;grid-template-columns:1fr 300px;}
78: 
79:     /* FLOW MAP */
80:     .map-section{position:relative;border-right:1px solid var(--border);}
81:     .map-tl-tag{
82:       position:absolute;top:12px;left:12px;z-index:10;
83:       background:rgba(7,10,15,.9);border:1px solid var(--border-s);padding:6px 10px;
84:       font-family:'Share Tech Mono',monospace;font-size:.62rem;color:var(--cyan);
85:     }
86:     .map-status-tags{position:absolute;top:12px;right:12px;z-index:10;display:flex;gap:6px;}
87:     .map-tag{padding:4px 10px;font-family:'Share Tech Mono',monospace;font-size:.62rem;text-transform:uppercase;letter-spacing:.08em;border:1px solid;}
88:     .tag-stable{color:var(--green);border-color:rgba(0,238,136,.4);background:var(--green-dim);}
89:     .tag-alarm{color:var(--red);border-color:rgba(255,51,68,.4);background:var(--red-dim);}
90:     #infraCanvas{width:100%;height:380px;display:block;}
91:     .map-footer-label{position:absolute;bottom:10px;left:12px;font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);text-transform:uppercase;letter-spacing:.1em;}
92: 
93:     /* RIGHT PANEL */
94:     .right-panel{display:flex;flex-direction:column;}
95: 
96:     /* VIBRATION CHART */
97:     .vib-section{padding:14px 16px;border-bottom:1px solid var(--border);}
98:     .panel-title{font-family:'Share Tech Mono',monospace;font-size:.62rem;letter-spacing:.14em;text-transform:uppercase;color:var(--text-dim);display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;}
99:     .vib-value{font-family:'Orbitron',monospace;font-size:1.4rem;font-weight:700;color:var(--cyan);}
100:     .vib-sub{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);}
101:     #vibCanvas{width:100%;height:100px;}
102:     .vib-time-labels{display:flex;justify-content:space-between;font-family:'Share Tech Mono',monospace;font-size:.56rem;color:var(--text-dim);margin-top:4px;}
103: 
104:     /* PRESSURE CHART */
105:     .pres-section{padding:14px 16px;flex:1;}
106:     #presCanvas{width:100%;height:90px;}
107:     .pres-labels{display:flex;justify-content:space-between;font-family:'Share Tech Mono',monospace;font-size:.56rem;color:var(--text-dim);margin-top:4px;}
108: 
109:     /* BOTTOM GRID */
110:     .bottom-grid{display:grid;grid-template-columns:1fr 1fr 1fr;border-top:1px solid var(--border);}
111:     .event-panel,.swarm-panel,.ebpf-panel{padding:14px 16px;border-right:1px solid var(--border);}
112:     .ebpf-panel{border-right:none;}
113: 
114:     /* EVENT LOG */
115:     .event-item{display:flex;gap:10px;padding:8px 0;border-bottom:1px solid var(--border);}
116:     .event-item:last-child{border-bottom:none;}
117:     .event-time{font-family:'Share Tech Mono',monospace;font-size:.62rem;color:var(--cyan);flex-shrink:0;text-align:right;min-width:36px;}
118:     .event-time-sub{font-family:'Share Tech Mono',monospace;font-size:.56rem;color:var(--text-dim);}
119:     .event-title{font-family:'Share Tech Mono',monospace;font-size:.68rem;color:#d8eaf8;font-weight:600;margin-bottom:2px;}
120:     .event-desc{font-size:.78rem;color:var(--text);line-height:1.4;}
121:     .event-icon{color:var(--amber);}
122: 
123:     /* SWARM PANEL */
124:     .swarm-grid{display:grid;grid-template-columns:1fr 1fr;gap:10px;}
125:     .swarm-unit{
126:       background:var(--surface);border:1px solid var(--border);padding:10px;
127:       display:flex;flex-direction:column;align-items:center;gap:6px;
128:     }
129:     .swarm-icon-ring{width:32px;height:32px;border-radius:50%;border:1px solid var(--border-s);display:flex;align-items:center;justify-content:center;position:relative;}
130:     .swarm-status-dot{position:absolute;top:-2px;right:-2px;width:7px;height:7px;border-radius:50%;background:var(--green);box-shadow:0 0 5px var(--green);}
131:     .swarm-unit-label{font-family:'Share Tech Mono',monospace;font-size:.62rem;color:#d8eaf8;font-weight:600;text-align:center;}
132:     .swarm-unit-sub{font-family:'Share Tech Mono',monospace;font-size:.56rem;color:var(--text-dim);text-align:center;line-height:1.4;}
133: 
134:     /* eBPF PANEL */
135:     .ebpf-header{display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;}
136:     .ebpf-version{font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--green);}
137:     .ebpf-log{font-family:'Share Tech Mono',monospace;font-size:.6rem;line-height:1.9;}
138:     .ebpf-line{color:var(--text-dim);}
139:     .ebpf-line .time{color:var(--text-dim);margin-right:6px;}
140:     .ebpf-line .msg{color:var(--text);}
141:     .ebpf-line .active{color:var(--green);}
142:     .ebpf-prompt{color:var(--cyan);}
143:     .ebpf-coords{display:flex;justify-content:space-between;font-family:'Share Tech Mono',monospace;font-size:.58rem;color:var(--text-dim);margin-top:10px;border-top:1px solid var(--border);padding-top:8px;}
144: 
145:     /* FOOTER */
146:     footer{position:fixed;bottom:0;left:200px;right:0;height:26px;background:var(--bg2);border-top:1px solid var(--border);display:flex;align-items:center;padding:0 16px;gap:12px;font-family:'Share Tech Mono',monospace;font-size:.6rem;}
147:     .live-dot{width:6px;height:6px;background:var(--green);border-radius:50%;animation:pulse-dot 1.4s ease-in-out infinite;}
148:     @keyframes pulse-dot{0%,100%{opacity:1}50%{opacity:.4}}
149:     ::-webkit-scrollbar{width:3px;} ::-webkit-scrollbar-track{background:var(--bg);} ::-webkit-scrollbar-thumb{background:var(--surface2);}
150:   </style>
151: </head>
152: <body>
153: 
154: <!-- HEADER -->
155: <header>
156:   <div class="brand">URIEL MONOLITH</div>
157:   <div class="header-nav">
158:     <div class="nav-tab">NODE HEALTH</div>
159:     <div class="nav-tab active">BWARI REGION</div>
160:     <div class="nav-tab">FCT SECTOR</div>
161:   </div>
162:   <div class="header-right">
163:     <div class="header-search"><span class="material-symbols-outlined" style="font-size:15px">search</span> QUERY SYSTEM…</div>
164:     <div class="icon-btn"><span class="material-symbols-outlined">sensors</span></div>
165:     <div class="icon-btn"><span class="material-symbols-outlined">hub</span></div>
166:     <div class="icon-btn"><span class="material-symbols-outlined">settings</span></div>
167:     <div class="icon-btn" style="border-radius:50%;background:var(--surface);border:1px solid var(--border-s)"><span class="material-symbols-outlined">person</span></div>
168:   </div>
169: </header>
170: 
171: <!-- SIDEBAR -->
172: <aside>
173:   <div class="strat-com-label">
174:     <div class="strat-com-bar"></div>
175:     <div>
176:       <div class="strat-com-text">STRAT-COM</div>
177:       <div class="strat-com-sub">V-01 S.W.A.R.M.</div>
178:     </div>
179:   </div>
180:   <div class="sidebar-divider"></div>
181:   <a href="/infra" class="sidebar-item active">
182:     <span class="material-symbols-outlined">construction</span> INFRASTRUCTURE RESILIENCE
183:   </a>
184:   <a href="/" class="sidebar-item">
185:     <span class="material-symbols-outlined">shield</span> TROOP DEFENSE
186:   </a>
187:   <a href="/agri" class="sidebar-item">
188:     <span class="material-symbols-outlined">agriculture</span> AGRI 4.0
189:   </a>
190:   <div class="deploy-btn-wrap">
191:     <button class="deploy-btn">DEPLOY ASSETS</button>
192:   </div>
193: </aside>
194: 
195: <!-- MAIN -->
196: <main>
197:   <!-- HERO -->
198:   <div class="infra-hero">
199:     <div class="infra-title">PROJECT CAESAR: BWARI RESILIENCE</div>
200:     <div class="infra-status-bar">
201:       <div class="status-item">
202:         <div class="status-dot" style="background:var(--green);box-shadow:0 0 6px var(--green)"></div>
203:         <span style="color:var(--text-dim)">GRID CONNECTIVITY:</span>
204:         <span style="color:var(--green)" id="gridConn">99.4%</span>
205:       </div>
206:       <div class="status-item">
207:         <div class="status-dot" style="background:var(--amber);box-shadow:0 0 6px var(--amber)"></div>
208:         <span style="color:var(--amber)">ANOMALY DETECTED: <span id="anomalyId">PIPE-S-12</span></span>
209:       </div>
210:       <div class="epoch-display">
211:         CURRENT EPOCH<br>
212:         <span class="epoch-value" id="epochVal">02:14:59:12 MS</span>
213:       </div>
214:     </div>
215:   </div>
216: 
217:   <!-- MAIN GRID -->
218:   <div class="main-grid">
219:     <!-- FLOW MAP -->
220:     <div class="map-section">
221:       <div class="map-tl-tag">
222:         RE-S-88 // SECTOR 4A<br>LAT: 9.2842 // LON: 7.3820
223:       </div>
224:       <div class="map-status-tags">
225:         <div class="map-tag tag-stable">STABLE</div>
226:         <div class="map-tag tag-alarm" id="alarmTag">ALARM</div>
227:       </div>
228:       <canvas id="infraCanvas"></canvas>
229:       <div class="map-footer-label">INFRASTRUCTURE MESH COVERAGE</div>
230:     </div>
231: 
232:     <!-- RIGHT PANEL -->
233:     <div class="right-panel">
234:       <div class="vib-section">
235:         <div class="panel-title">
236:           VIBRATION ANOMALY PROBABILITY
237:           <span class="vib-value"><span id="vibVal">0.024</span> <span style="font-family:'Rajdhani';font-size:.75rem;color:var(--text-dim)">RMS</span></span>
238:         </div>
239:         <canvas id="vibCanvas"></canvas>
240:         <div class="vib-time-labels"><span>T-10m</span><span>T-5m</span><span>CURRENT</span></div>
241:       </div>
242:       <div class="pres-section">
243:         <div class="panel-title">
244:           PIPELINE PRESSURE
245:           <span style="font-family:'Orbitron',monospace;font-size:1.2rem;font-weight:700;color:var(--cyan)"><span id="presVal">14.2</span> BAR</span>
246:         </div>
247:         <canvas id="presCanvas"></canvas>
248:         <div class="pres-labels">
249:           <span>0.0 BAR</span><span>NOMINAL: 14.5</span><span>MAX: 20.0</span>
250:         </div>
251:       </div>
252:     </div>
253:   </div>
254: 
255:   <!-- BOTTOM -->
256:   <div class="bottom-grid">
257:     <!-- EVENTS -->
258:     <div class="event-panel">
259:       <div class="panel-title" style="margin-bottom:10px">
260:         <span style="display:flex;align-items:center;gap:6px">
261:           <span class="material-symbols-outlined event-icon" style="font-size:16px">warning</span>
262:           CRITICAL EVENT LOGS
263:         </span>
264:       </div>
265:       <div id="eventLog"></div>
266:     </div>
267: 
268:     <!-- SWARM -->
269:     <div class="swarm-panel">
270:       <div class="panel-title" style="margin-bottom:12px">
271:         <span style="display:flex;align-items:center;gap:6px">
272:           <span class="material-symbols-outlined" style="font-size:16px">precision_manufacturing</span>
273:           MAINTENANCE SWARM
274:         </span>
275:       </div>
276:       <div class="swarm-grid" id="swarmGrid"></div>
277:     </div>
278: 
279:     <!-- eBPF -->
280:     <div class="ebpf-panel">
281:       <div class="ebpf-header">
282:         <div class="panel-title" style="margin-bottom:0">
283:             <span style="display:flex;align-items:center;gap:6px">
284:             <span class="material-symbols-outlined" style="font-size:15px">terminal</span>
285:             URIEL KERNEL TELEMETRY
286:           </span>
287:         </div>
288:         <div class="ebpf-version" id="ebpfVersion">v2.4.9-STABLE</div>
289:       </div>
290:       <div class="ebpf-log" id="ebpfLog"></div>
291:       <div class="ebpf-prompt">&gt; LISTENING ON INTERFACE ETH_RE_0…</div>
292:       <div class="ebpf-coords">
293:         <span>LAT: 9.2842 N</span>
294:         <span>LON: 7.3820 E</span>
295:         <span>ALT: 1,244m</span>
296:       </div>
297:     </div>
298:   </div>
299: </main>
300: 
301: <!-- FOOTER -->
302: <footer>
303:   <div class="live-dot"></div>
304:   <span style="color:var(--green)">SYSTEM NOMINAL</span>
305:   <span style="color:var(--text-dim)">|</span>
306:   <span style="color:var(--text-dim)">INFRA-LINK ACTIVE</span>
307:   <span style="color:var(--text-dim)">|</span>
308:   <span style="color:var(--text-dim)">DEFENSE-GRID: SECURE</span>
309:   <span style="margin-left:auto;color:var(--text-dim)">ENC: AES-256-XTS</span>
310: </footer>
311: 
312: <script src="/static/ceasar-api.js"></script>
313: <script src="/static/infra.js"></script>
314: </body>
315: </html>
316: """

def process_and_replace(raw_str, replacements):
    lines = raw_str.split("\n")
    cleaned = [re.sub(r'^\d+: ', '', line) for line in lines]
    text = "\n".join(cleaned)
    for k, v in replacements.items():
        text = text.replace(k, v)
    return text

agri_replacements = {
    'id="heroIntegrity">99.82%': 'id="heroIntegrity">--',
    'id="heroStatus">NOMINAL': 'id="heroStatus">--',
    'id="heroSwarm">ACTIVE: 128': 'id="heroSwarm">ACTIVE: --',
    'id="maxYield">8.42 t/ha': 'id="maxYield">-- t/ha',
    'id="soilMoisture">62.4': 'id="soilMoisture">--',
    'id="soilBar" style="width:62.4%"': 'id="soilBar" style="width:0%"',
    'id="ndviVal">0.82': 'id="ndviVal">--',
    'id="ndviOverlap">80% OVERLAP': 'id="ndviOverlap">-- OVERLAP',
    'id="ndviBar" style="width:82%"': 'id="ndviBar" style="width:0%"',
    'id="humidVal">84.1': 'id="humidVal">--',
    'id="humidDelta">2.4%': 'id="humidDelta">--',
    'id="humidBar" style="width:84.1%;': 'id="humidBar" style="width:0%;',
    'id="astZone">QUADRANT-DELTA': 'id="astZone">--',
    'id="waterSavings">+12,400L / WK': 'id="waterSavings">--',
    'id="astEff">92%': 'id="astEff">--',
    'id="astEffBar" style="width:92%"': 'id="astEffBar" style="width:0%"',
    'id="fedEpoch">42': 'id="fedEpoch">--',
    'id="fedLossVal">0.0021': 'id="fedLossVal">--',
    'id="fedConv">0.994': 'id="fedConv">--',
    'id="fedConfidence">0.982': 'id="fedConfidence">--',
    'id="fedNodes">312 / 312': 'id="fedNodes">-- / --',
    'id="vBat1" style="width:88%"': 'id="vBat1" style="width:0%"',
    'id="vBat1Val">88%': 'id="vBat1Val">--',
    'id="vSig2">-42dBm': 'id="vSig2">--'
}

infra_replacements = {
    'id="gridConn">99.4%': 'id="gridConn">--',
    'id="anomalyId">PIPE-S-12': 'id="anomalyId">--',
    'id="epochVal">02:14:59:12 MS': 'id="epochVal">--',
    'id="vibVal">0.024': 'id="vibVal">--',
    'id="presVal">14.2': 'id="presVal">--',
    'id="ebpfVersion">v2.4.9-STABLE': 'id="ebpfVersion">--'
}

base_dir = r"c:\Users\USER\Documents\Previous\New project\services\caesar_console\static"
with open(os.path.join(base_dir, "agri.html"), "w", encoding="utf-8") as f:
    f.write(process_and_replace(agri_raw, agri_replacements))

with open(os.path.join(base_dir, "infra.html"), "w", encoding="utf-8") as f:
    f.write(process_and_replace(infra_raw, infra_replacements))
print("DONE")
