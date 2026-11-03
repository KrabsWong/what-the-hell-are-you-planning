const media=matchMedia('(prefers-color-scheme: dark)');
let theme='auto';try{theme=localStorage.getItem('krabs-report-theme')||'auto'}catch(e){}
if(!['auto','light','dark'].includes(theme))theme='auto';
function applyTheme(){document.documentElement.dataset.theme=theme==='auto'?(media.matches?'dark':'light'):theme;document.querySelectorAll('[data-icon]').forEach(e=>e.hidden=e.dataset.icon!==theme);document.querySelectorAll('[data-theme-value]').forEach(e=>e.setAttribute('aria-pressed',String(e.dataset.themeValue===theme)))}
applyTheme();media.addEventListener('change',applyTheme);
document.querySelectorAll('[data-theme-value]').forEach(b=>b.addEventListener('click',()=>{theme=b.dataset.themeValue;try{localStorage.setItem('krabs-report-theme',theme)}catch(e){}applyTheme();b.closest('details').open=false;b.closest('details').querySelector('summary').focus()}));
document.addEventListener('click',e=>document.querySelectorAll('.theme-picker[open]').forEach(d=>{if(!d.contains(e.target))d.open=false}));
document.addEventListener('keydown',e=>{if(e.key==='Escape')document.querySelectorAll('.theme-picker[open]').forEach(d=>{d.open=false;d.querySelector('summary').focus()})});
document.getElementById('toc-search').addEventListener('input',e=>document.querySelectorAll('.sidebar nav a').forEach(a=>a.hidden=!a.textContent.toLowerCase().includes(e.target.value.toLowerCase())));
const observer=new IntersectionObserver(entries=>entries.forEach(e=>{if(e.isIntersecting)document.querySelectorAll('.sidebar nav a').forEach(a=>a.classList.toggle('active',a.getAttribute('href')==='#'+e.target.id))}),{rootMargin:'0px 0px -65% 0px'});
document.querySelectorAll('main h1[id], main h2[id], main h3[id]').forEach(e=>observer.observe(e));
