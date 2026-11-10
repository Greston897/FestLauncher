const $ = (s) => document.querySelector(s);
const defaults = {username:'Player',version:'26.2',memory:4,java:'java',mods:['fabric-api','sodium','iris']};
let settings = {...defaults}, connected = false, busy = false, saveQueue = Promise.resolve();
const catalog = [
  {id:'fabric-api', name:'Fabric API', description:'Основа для работы модов Fabric', icon:'F'},
  {id:'sodium', name:'Sodium', description:'Больше кадров и плавности', icon:'S'},
  {id:'iris', name:'Iris Shaders', description:'Поддержка шейдеров и красивого освещения', icon:'✳'},
  {id:'visuality', name:'Visuality', description:'Атмосферные частицы и детали', icon:'V'}
];
function page(name) {
  if (!['home','mods','settings'].includes(name)) name = 'home';
  document.querySelectorAll('.page').forEach(x => x.classList.toggle('active-page',x.id === `page-${name}`));
  document.querySelectorAll('.nav').forEach(x => x.classList.toggle('active',x.dataset.page === name));
  $('#breadcrumb').textContent = {home:'Главная',mods:'Моды',settings:'Настройки'}[name];
  history.replaceState(null,'',`#${name}`); window.scrollTo(0,0);
}
document.querySelectorAll('[data-page]').forEach(el => el.addEventListener('click', () => page(el.dataset.page)));
window.addEventListener('hashchange',()=>page(location.hash.slice(1)));
function renderMods(){
  $('#modsList').replaceChildren(...catalog.map(mod => {
    const row = document.createElement('div'); row.className='mod-row';
    const logo=document.createElement('div'); logo.className='mod-logo'; logo.textContent=mod.icon;
    const text=document.createElement('div'); text.className='mod-text';
    const title=document.createElement('strong'); title.textContent=mod.name;
    const description=document.createElement('small'); description.textContent=mod.description; text.append(title,description);
    const label=document.createElement('label'); label.className='switch';
    const input=document.createElement('input'); input.type='checkbox'; input.checked=settings.mods.includes(mod.id); input.setAttribute('aria-label',mod.name);
    input.addEventListener('change',()=>{
      let mods=new Set(settings.mods); input.checked ? mods.add(mod.id) : mods.delete(mod.id);
      if(mod.id==='iris' && input.checked) mods.add('sodium');
      if(mod.id==='sodium' && !input.checked) mods.delete('iris');
      settings.mods=[...mods]; renderMods(); persist();
    });
    label.append(input,document.createElement('span')); row.append(logo,text,label); return row;
  }));
}
function render(){
  $('#username').value=settings.username; $('#homeUsername').value=settings.username; $('#java').value=settings.java; $('#memory').value=settings.memory; $('#ramValue').textContent=settings.memory;
  renderAccount();
  const select=$('#version'); if (![...select.options].some(x=>x.value===settings.version)) select.add(new Option(settings.version,settings.version)); select.value=settings.version;
  renderMods();
}
function connection(isConnected){connected=isConnected; $('#connection').textContent=isConnected?'Лаунчер подключён':'Нет подключения к ядру'; $('.online-dot').style.background=isConnected?'#ff883d':'#c88e82'; $('#launch').disabled=!isConnected||busy;}
async function api(url,options){const response=await fetch(url,options); if(!response.ok) throw new Error((await response.text())||`Ошибка ${response.status}`); return response.json();}
function persist(){
  if(!connected){$('#saveMessage').textContent='Запустите Rust-ядро, чтобы сохранить настройки'; $('#statusText').textContent='Настройки не сохранены: Rust-ядро не подключено.'; return Promise.resolve(false);}
  const snapshot=JSON.stringify(settings);
  saveQueue=saveQueue.catch(()=>{}).then(async()=>{try{await api('/api/settings',{method:'POST',headers:{'Content-Type':'application/json'},body:snapshot}); $('#saveMessage').textContent='Настройки сохранены'; return true;}catch(e){$('#saveMessage').textContent=e.message; $('#statusText').textContent=e.message; return false;}});
  return saveQueue;
}
$('#homeUsername').addEventListener('change',e=>{
  const username=e.target.value.trim();
  if(!/^[A-Za-z0-9_]{3,16}$/.test(username)){ $('#statusText').textContent='Ник: 3–16 латинских букв, цифр или _'; e.target.value=settings.username; return; }
  settings.username=username; $('#username').value=username; persist();
});
$('#homeUsername').addEventListener('keydown',e=>{if(e.key==='Enter') e.target.blur();});
$('#version').addEventListener('change',e=>{settings.version=e.target.value;persist();});
$('#memory').addEventListener('input',e=>{$('#ramValue').textContent=e.target.value;});
$('#save').addEventListener('click',async()=>{
  const username=$('#username').value.trim(), java=$('#java').value.trim();
  if(!/^[A-Za-z0-9_]{3,16}$/.test(username)){ $('#saveMessage').textContent='Ник: 3–16 латинских букв, цифр или _'; return; }
  if(!java){$('#saveMessage').textContent='Укажите путь к Java';return;}
  settings={...settings,username,java,memory:Number($('#memory').value)}; render(); await persist();
});
$('#launch').addEventListener('click',async()=>{
  if(!(await persist())) return;
  try{await api('/api/launch',{method:'POST'}); await refreshStatus();}catch(e){$('#statusText').textContent=e.message;}
});
async function refreshStatus(){try{const s=await api('/api/status');busy=s.busy;connection(true);$('#statusText').textContent=s.message;$('#launch span:nth-child(2)').textContent=s.busy?(s.phase==='running'?'В игре':'Подготовка'):'Играть';$('.status-dot').style.background=s.phase==='error'?'#d38f84':'#ff883d';}catch{connection(false);$('#statusText').textContent='Запустите Rust-ядро, чтобы установить и запустить игру.';}}

// This is intentionally a UI-only demo, not an authentication service.
// Passwords never leave the form and are cleared on every transition.
const authOverlay = $('#authOverlay');
let authMode = 'login';
let demoName = sessionStorage.getItem('fest-demo-name') || '';
let lastAuthFocus = null;
function renderAccount(){
  $('#sideName').textContent=demoName || 'Гость';
  $('#topName').textContent=demoName || 'Войти';
  $('#sideAccountHint').textContent=demoName ? 'Демо-сессия' : 'Демо-аккаунт';
}
function clearAuthSecrets(){ $('#authPassword').value=''; $('#authConfirm').value=''; }
function authTab(mode){
  authMode=mode; const register=mode==='register';
  $('#loginTab').classList.toggle('active',!register); $('#registerTab').classList.toggle('active',register);
  $('#loginTab').setAttribute('aria-selected',String(!register)); $('#registerTab').setAttribute('aria-selected',String(register));
  $('#authTitle').innerHTML=register?'Создать аккаунт<span>.</span>':'С возвращением<span>.</span>';
  $('#authSubtitle').textContent=register?'Создай демонстрационный профиль.':'Войди, чтобы продолжить.';
  $('#authSubmitText').textContent=register?'Создать профиль':'Войти';
  $('#confirmWrap').hidden=!register; $('#authConfirm').required=register;
  $('#authError').textContent=''; clearAuthSecrets();
}
function openAuth(){ lastAuthFocus=document.activeElement; authTab('login'); $('#authName').value=demoName; authOverlay.hidden=false; document.body.style.overflow='hidden'; $('#authName').focus(); }
function closeAuth(){ authOverlay.hidden=true; document.body.style.overflow=''; clearAuthSecrets(); $('#authName').value=''; $('#authError').textContent=''; sessionStorage.setItem('fest-auth-dismissed','1'); if(lastAuthFocus) lastAuthFocus.focus(); }
$('#openAccount').addEventListener('click',openAuth);
$('#openAccountSide').addEventListener('click',openAuth);
$('#authClose').addEventListener('click',closeAuth);
$('#authGuest').addEventListener('click',()=>{demoName='';sessionStorage.removeItem('fest-demo-name');renderAccount();closeAuth();});
$('#loginTab').addEventListener('click',()=>authTab('login'));
$('#registerTab').addEventListener('click',()=>authTab('register'));
$('#authForm').addEventListener('submit',e=>{
  e.preventDefault(); const name=$('#authName').value.trim();
  if(!name){$('#authError').textContent='Введи логин.';return;}
  if(authMode==='register' && $('#authPassword').value!==$('#authConfirm').value){$('#authError').textContent='Пароли не совпадают.';return;}
  demoName=name; sessionStorage.setItem('fest-demo-name',demoName); renderAccount(); closeAuth();
});
document.addEventListener('keydown',e=>{
  if(authOverlay.hidden) return;
  if(e.key==='Escape'){ closeAuth(); return; }
  if(e.key==='Tab'){
    const focusable=[...authOverlay.querySelectorAll('button, input')].filter(el=>!el.disabled && !el.closest('[hidden]'));
    const first=focusable[0], last=focusable[focusable.length-1];
    if(e.shiftKey && document.activeElement===first){e.preventDefault();last.focus();}
    else if(!e.shiftKey && document.activeElement===last){e.preventDefault();first.focus();}
  }
});

async function init(){
  page(location.hash.slice(1)||'home'); render();
  if(!sessionStorage.getItem('fest-auth-dismissed')) openAuth();
  try{settings=await api('/api/settings');connection(true);render();}catch{connection(false);}
  refreshStatus();
  try{const versions=await api('/api/versions'); const select=$('#version'); for(const v of versions) if(![...select.options].some(o=>o.value===v)) select.add(new Option(v,v)); select.value=settings.version;}catch{/* The selected version remains available without a network connection. */}
  setInterval(refreshStatus,3000);
}
init();
