#!/usr/bin/env python3
from pathlib import Path
from playwright.sync_api import sync_playwright
import json,hashlib,re,sys,os
R=Path(__file__).resolve().parents[1]; checks=[];errors=[];requests=[]
def ck(name, value, detail=''):
 checks.append({'name':name,'pass':bool(value),'detail':detail})
def nav(p,route):
 p.evaluate('(h)=>{location.hash=h}',route);p.wait_for_timeout(40)
with sync_playwright() as pw:
 browser=pw.chromium.launch(headless=True,args=['--no-sandbox'],**({'executable_path':os.environ['ZIL_BROWSER_EXECUTABLE']} if os.environ.get('ZIL_BROWSER_EXECUTABLE') else {}))
 ctx=browser.new_context(viewport={'width':1440,'height':1000},accept_downloads=True)
 p=ctx.new_page();p.on('pageerror',lambda e:errors.append(str(e)));p.on('request',lambda r:requests.append(r.url))
 mode='file://';fallback=None
 try:
  p.goto((R/'lang-designer.html').as_uri(),wait_until='load',timeout=15000)
 except Exception as e:
  fallback=str(e);mode='set_content on fresh about:blank';p.close();p=ctx.new_page();p.on('pageerror',lambda e:errors.append(str(e)));p.on('request',lambda r:requests.append(r.url));p.set_content((R/'lang-designer.html').read_text(),wait_until='load')
 p.wait_for_function('globalThis.DesignAtlas && document.querySelectorAll("input[name=language]").length===8')
 ck('eight language checkboxes',p.locator('input[name=language]').count()==8)
 ck('initial language selection',p.locator('input[name=language]:checked').count()==3)
 ck('42 scene targets',p.evaluate('DesignAtlas.data.goals.length')==42)
 ck('28 variable dimensions',p.evaluate('DesignAtlas.data.variables.length')==28)
 ck('40 named rules',p.evaluate('DesignAtlas.data.rules.length')==40)
 ck('336 scene-language records',p.evaluate('DesignAtlas.data.coverage.length')==336)
 ck('initial scene has SVG flow',p.locator('svg.flow-svg').count()==1)
 ck('current version digest visible',hashlib.sha256((R/'_zil_design.md').read_bytes()).hexdigest()[:12] in p.locator('.edition').inner_text())
 p.locator('[data-action=select-all]').click();ck('all languages selectable together',p.locator('input[name=language]:checked').count()==8 and p.locator('.route-card').count()==8)
 p.locator('[data-action=clear-selection]').click();ck('empty language state is explicit','选择要比较的语言' in p.locator('#main').inner_text())
 p.locator('[data-action=select-default]').first.click();ck('default languages restored',p.locator('.route-card').count()==3)
 p.locator('[data-set=sceneMode][data-value=cards]').click();ck('card layout switch',p.locator('svg.flow-svg').count()==0 and p.locator('.route-card').count()==3)
 p.locator('[data-set=sceneMode][data-value=rules]').click();ck('rule table has human headers',p.locator('.table-wrap').count()>0 and '规则' in p.locator('.table-wrap').first.inner_text())
 p.locator('[data-set=sceneMode][data-value=flow]').click()
 p.locator('[data-rule=R37]').first.click() if p.locator('[data-rule=R37]').count() else p.locator('[data-rule]').first.click()
 ck('rule detail dialog opens',p.locator('#detail-dialog').evaluate('(e)=>e.open'))
 p.locator('#dialog-close').click();ck('dialog closes',not p.locator('#detail-dialog').evaluate('(e)=>e.open'))
 p.locator('#language-filter').select_option('zil');ck('proposal not shown as implemented','已有方案”不包含' in p.locator('#main').inner_text())
 p.locator('#include-proposals').check();ck('explicit proposal filter includes all 42',p.locator('[data-nav-type=goal]').count()==42)
 p.locator('#catalog-search').fill('编译');ck('scene search filters',0<p.locator('[data-nav-type=goal]').count()<42)
 p.locator('#catalog-search').fill('');p.locator('#language-filter').select_option('all')
 nav(p,'#basics/R37?langs=zil,rust');ck('new ordered rule is reachable','操作顺序决定同源调用条件' in p.locator('#main').inner_text())
 nav(p,'#basics/V02?langs=zil,rust');ck('variable current setting present','描述复制' in p.locator('#main').inner_text())
 nav(p,'#compare/variables?langs=zil,rust,zig');ck('variable matrix has all selected languages',p.locator('thead th').count()==4)
 with p.expect_download() as info:p.locator('[data-action=export-csv]').click()
 csv=info.value;csv.save_as(R/'evidence/exported-variables.csv');ck('CSV export contains updated setting','描述复制' in (R/'evidence/exported-variables.csv').read_text())
 nav(p,'#compare/goals?langs=zil,rust');ck('goals matrix includes all targets',p.locator('tbody tr').count()==42)
 nav(p,'#docs/spec-15?langs=zil,rust');ck('new syntax chapter loaded','固定源面与歧义消除' in p.locator('.doc').inner_text())
 nav(p,'#docs/spec-16?langs=zil,rust');ck('comptime chapter loaded','#' in p.locator('.doc').inner_text() and '编译期入口' in p.locator('.doc').inner_text())
 nav(p,'#docs/spec-17?langs=zil,rust');ck('attribute chapter loaded','属性与编译阶段' in p.locator('.doc').inner_text())
 ck('no horizontal overflow desktop',p.evaluate('document.documentElement.scrollWidth<=innerWidth+1'))
 p.wait_for_timeout(1800);p.screenshot(path=str(R/'evidence/desktop.png'),full_page=False)
 with p.expect_download() as info:p.locator('.top-download').click()
 dl=info.value;dl.save_as(R/'evidence/downloaded-design.md');ck('download is byte-identical current spec',(R/'evidence/downloaded-design.md').read_bytes()==(R/'_zil_design.md').read_bytes())
 for page,phrase in [('goal','施工总计划'),('workflow','每类信息只有一个定义源'),('syntax','文法产生式'),('changes','审查'),('sources','资料'),('guide','阅读')]:
  nav(p,'#docs/'+page+'?langs=zil,rust');ck('documentation '+page,phrase in p.locator('#main').inner_text())
 nav(p,'#docs/goal?langs=zil,rust')
 with p.expect_download() as info:p.locator('[data-action=download-doc]').click()
 info.value.save_as(R/'evidence/downloaded-goal.md');ck('goal download exact',(R/'evidence/downloaded-goal.md').read_bytes()==(R/'_zil_goal.md').read_bytes())
 # Navigation/render for every current normative chapter. These are UI checks, not ZIL execution.
 allids=p.evaluate('DesignAtlas.specChapters.map(c=>c.id)')
 for c in allids:
  nav(p,'#docs/spec-'+c+'?langs=zil,rust')
  ck('norm chapter '+c+' renders',len(p.locator('.doc').inner_text())>100)
 # Local file reload is only counted if actual file navigation succeeded.
 if mode=='file://':
  nav(p,'#docs/spec-17?langs=zil,rust');p.reload(wait_until='load');ck('file deep-link reload works','属性与编译阶段' in p.locator('.doc').inner_text())
 p.set_viewport_size({'width':390,'height':844});nav(p,'#scenes/G09?langs=zil,rust')
 ck('no mobile viewport overflow',p.evaluate('document.documentElement.scrollWidth<=innerWidth+1'))
 p.locator('[data-action=toggle-catalog]').first.click();ck('mobile catalog expands',p.locator('.sidebar-list').first.is_visible())
 p.locator('[data-nav-type=goal]').first.click();ck('mobile catalog selection collapses',not p.locator('.sidebar-list').first.is_visible())
 nav(p,'#docs/spec-17?langs=zil,rust');ck('mobile attribute documentation renders','属性与编译阶段' in p.locator('.doc').inner_text())
 ck('mobile docs no viewport overflow',p.evaluate('document.documentElement.scrollWidth<=innerWidth+1'))
 p.wait_for_timeout(1800);p.screenshot(path=str(R/'evidence/mobile.png'),full_page=False)
 ck('no JavaScript page errors',not errors,str(errors))
 external=[r for r in requests if r.startswith(('https:','http:'))];ck('no network assets required',not external,str(external))
 version=browser.version;browser.close()
report={'scope':'Offline HTML rendering, interactions and exact downloads; no compiler or language-safety verification','browser':version,'mode':mode,'fallback':fallback,'total':len(checks),'passed':sum(x['pass'] for x in checks),'checks':checks,'page_errors':errors,'external_requests':external}
(R/'evidence/browser.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(json.dumps(report,ensure_ascii=False,indent=2));sys.exit(0 if report['passed']==report['total'] else 1)
