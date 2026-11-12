// Content of the user guide (docs/user-guide/PHN-MCP-Server-User-Guide.docx).
//
// Edit this file to change what the guide says; build-user-guide.js turns it into Word.
// Inline markup: **bold** and `code`. Use curly quotes and real dashes in prose.
//
// Only tools that are implemented and registered in crates/phn-server belong here. When a
// tool is added, add it to the right chapter below and to the counts in the front matter,
// then rebuild (see README.md in this folder).

'use strict';

const VERSION = '0.15.0';
const GUIDE_DATE = 'November 2026';
const TOOL_COUNT = 121;

const TOOLSETS = {
  core: 'core (always on)',
  cd: 'cd (communicable disease)',
  vax: 'vax (immunizations)',
  epi: 'epi (epidemiology and statistics)',
  equity: 'equity (community health and equity)',
  chronic: 'chronic (chronic disease)',
  ebp: 'ebp (evidence-based practice and quality improvement)',
  comms: 'comms (communications)',
  school: 'school (school health)',
  mch: 'mch (maternal, child, and family health)',
  aging: 'aging (healthy aging and older adults)',
  bh: 'bh (mental health and substance use)',
  epr: 'epr (emergency preparedness)',
  env: 'env (environmental health and climate)',
};

const INTERNET = {
  no: 'not needed',
  optional: 'used when available; falls back to a bundled or cached copy',
  yes: 'needed',
};

// ---------------------------------------------------------------------------------------
// Front matter: chapters 1–3. Block types: p, bullets, steps, table, example, note.
// ---------------------------------------------------------------------------------------

const frontChapters = [
  {
    title: 'Executive summary',
    intro: [
      {
        p: 'Public health nurses spend a large part of each day looking things up and doing careful calculations. Is this lab result reportable, and how fast? When can this child return to school? Which kindergartens in town are below 95% for MMR? How do I age-adjust these rates for the community health assessment? Each answer is usually somewhere in a regulation, a DPH guidance document, a CDC dataset, or a spreadsheet on your own computer, and finding it takes time.',
      },
      {
        p: `The **Public Health Nursing MCP Server** is a free, open-source add-on for AI assistants such as Claude. It gives the assistant ${TOOL_COUNT} tools built for public health nursing practice in Massachusetts. You ask in plain language. The assistant uses the tools to find the answer in Massachusetts regulations, DPH guidance, CDC and Census data, and your own files, and it shows you where each answer came from. (MCP stands for Model Context Protocol, the open standard that lets an AI assistant use outside tools.)`,
      },
    ],
    sections: [
      {
        title: 'Why use it',
        blocks: [
          {
            bullets: [
              '**Faster answers to everyday questions.** Reportable conditions under 105 CMR 300.000, isolation and exclusion dates, school immunization requirements, vaccine products, and passages from DPH’s *Guide to Surveillance, Reporting and Control* come back in seconds, with the section and date cited.',
              '**Calculations you can trust and defend.** Rates, confidence intervals, age adjustment, 2×2 tables, trend analysis, outbreak signals, sample sizes, and immunization forecasts are computed by tested program code, not estimated by the AI. Automated tests compare the results with published reference values and standard statistical software (R and SciPy), and the immunization forecaster passes all 1,064 of CDC’s test cases for healthy children and adults. Every answer states the method it used.',
              '**Built for Massachusetts.** The tools know 105 CMR 300.000, DPH’s disease guide and fact sheets, DPH’s school immunization requirements, the DPH school immunization survey, and all 351 cities and towns, 14 counties, HMCC regions, census tracts, and school districts.',
              '**Privacy by design.** The server runs on your own computer. Your files never leave it. Tools that read your data send the AI only counts and statistics, apply small-cell suppression automatically, and save anything identifiable (letters, rosters, per-student results) to your output folder instead of showing it in the chat. Redaction cannot be turned off, and the server sends no usage data anywhere.',
              '**Sources and dates on every answer.** Each answer lists its sources with dates and says whether the information came live from the internet, from a recent cached copy, from a dated snapshot bundled with the server, or from your own files.',
              '**Works offline.** Most tools use bundled reference content and dated data snapshots, so they keep working when the internet is slow, blocked, or down.',
              '**Equity and accessibility built in.** Community health profiles, the Social Vulnerability Index, language access checks, and disparity measures are one request away, and the documents, charts, and maps the server creates are designed to meet Section 508 and WCAG 2.1 AA.',
              '**Finished work products.** The server saves accessible Word and PDF documents on your letterhead, Excel workbooks, charts, maps, epidemic curves, contact-tracing workbooks, clinic reconciliation sheets, and mail-merged letters to your output folder.',
            ],
          },
        ],
      },
      {
        title: 'What you can do with it',
        blocks: [
          { p: `The ${TOOL_COUNT} tools fall into 14 work areas. Chapters 4 to 17 describe each tool.` },
          {
            table: {
              caption: 'Table 1. Work areas covered by the server',
              widths: [2200, 5560, 1600],
              header: ['Work area', 'Examples of what you can do', 'Tools'],
              rows: [
                ['Setup and data sources (chapter 4)', 'Save your town, letterhead, and preferences; see how current each data source is.', '2'],
                ['Your files and documents (chapter 5)', 'Read and search your files and policies; describe and de-identify datasets; suppress small numbers; create accessible documents and mail-merged letters; check accessibility.', '7'],
                ['Communicable disease (chapter 6)', 'Check reportability; calculate isolation, quarantine, and exclusion dates; assess measles exposures; plan contact tracing; analyze outbreaks and draw epi curves; spot unusual increases; follow national notifiable disease and respiratory and wastewater trends.', '11, plus a guided outbreak investigation'],
                ['Immunizations (chapter 7)', 'Look up schedules and compare DPH, professional society, and CDC recommendations; forecast doses due; check students against school requirements; rank school coverage; handle temperature excursions; plan clinics; prepare for vaccine-hesitancy conversations; look up medications.', '10'],
                ['Epidemiology and statistics (chapter 8)', 'Rates with confidence intervals, age adjustment, SIR and SMR, 2×2 tables, statistical tests, sample size, trends, charts, control charts, and maps.', '10'],
                ['Community health and equity (chapter 9)', 'One-request community health profiles, Social Vulnerability Index, language access, disparity measures, and CDC PLACES chronic disease estimates.', '5'],
                ['Evidence-based practice (chapter 10)', 'PICO-based PubMed searches and the newest MMWR reports and Health Alert Network notices.', '2'],
                // Phase 7 work-area rows go below this line.
                ['School health (chapter 11)', 'Screening requirements and tracking with referral letters; medication administration rules; individualized health plans; concussion return to learn; illness absenteeism; the year-end report; child care health consultation.', '8'],
                // End of Phase 7 work-area rows.
                // Phase 8 work-area rows go below this line.
                ['Maternal, child, and family health (chapter 12)', 'Growth percentiles with corrected age and extended BMI; lead screening, follow-up, and family teaching; mandated reporting (51A) with a written-report draft; developmental milestones and Early Intervention referral; newborn and postpartum visit plans; benefit eligibility screening; lead outreach maps; maternal and infant health profiles.', '8'],
                // End of Phase 8 work-area rows.
                // Phase 9 work-area rows go below this line.
                ['Public health nursing practice (chapter 13)', 'Plan TB contact investigations and latent TB treatment; screen for social needs and find community resources; navigate newcomer health and coverage; code notes with the Omaha System and the Intervention Wheel; use documentation templates.', '8'],
                // End of Phase 9 work-area rows.
                // Phase 10 work-area rows go below this line.
                ['Healthy aging and mental health screening (chapter 14)', 'Score depression, anxiety, perinatal, and suicide risk screens with safety alerts; find crisis and aging services by town; assess fall risk; report elder abuse; explain coverage; look up nursing home quality; map older adults at risk; plan advance care planning sessions.', '9'],
                // End of Phase 10 work-area rows.
                // Phase 11 work-area rows go below this line.
                ['Substance use, crisis resources, and health communication (chapter 15)', 'Check reading level and plain language; look up overdose deaths and youth survey data; find naloxone, harm reduction, and treatment; plan a school’s response after a suicide; prepare and check translations; plan social media campaigns; draft press materials.', '9'],
                // End of Phase 11 work-area rows.
                // Phase 12 work-area rows go below this line.
                ['Emergency preparedness and environmental health (chapter 16)', 'Check hazard alerts and their health effects; model dispensing sites; run a hazard vulnerability assessment; plan shelter intake for functional needs; find people who rely on powered medical equipment; plan a CASPER; fill ICS forms and Incident Action Plans; draft a continuity of operations plan; design exercises and write after-action reports; build crisis and emergency risk communication; check air quality; plan the heat response.', '13, plus a guided tabletop exercise'],
                // End of Phase 12 work-area rows.
                // Phase 14 work-area rows go below this line.
                ['Environmental health and climate (chapter 17)', 'Look up drinking water violations, lead, and PFAS for your town’s water systems; check community water fluoridation; screen environmental justice populations; profile climate and health vulnerability for MVP and hazard mitigation planning; follow mosquito and tick-borne disease risk; explain housing conditions under the State Sanitary Code; look up chemical exposure limits; map food access.', '8'],
                // End of Phase 14 work-area rows.
              ],
            },
          },
        ],
      },
      {
        title: 'Important limits',
        blocks: [
          {
            bullets: [
              '**Decision support only.** The server supports your professional judgment; it does not replace it, your agency’s policies, your medical director’s standing orders, or consultation with DPH. Verify before you act: for example, check immunization histories against the MIIS record, and report cases through MAVEN as DPH directs.',
              '**Reference content is a draft pending expert review.** The bundled reportable-condition list, disease rules, immunization schedules and recommendations, school requirements, and similar content were drafted from the source documents but have not yet been reviewed by the project’s public health nursing subject matter expert. Every answer built on this content says so.',
              '**Automated privacy protection can miss things.** Identifier detection is careful but not perfect. Review anything before you share it, and follow your agency’s policy on using AI tools.',
              '**What you type goes to the AI company.** The server protects the files in your data folder, but anything you type into the chat is sent to the AI vendor. Do not type names, record numbers, or other identifiers into the chat; put identifiable data in a file and let the tools work with it.',
              '**Nothing is submitted anywhere.** No tool submits reports or records to MAVEN, the MIIS, VAERS, or any other system.',
            ],
          },
        ],
      },
      {
        title: 'Who this guide is for',
        blocks: [
          {
            p: 'This guide is written for public health nurses: board of health and communicable disease nurses, school nurses, nurse leaders, and preparedness staff in Massachusetts. It assumes you know public health nursing practice but not the technology. It describes version ' + VERSION + ' of the server. Installation is usually done once, with help from your IT department; chapter 3 summarizes it, and the project’s README file has the technical details.',
          },
        ],
      },
    ],
  },
  {
    title: 'How the server works',
    intro: [],
    sections: [
      {
        title: 'You ask; the assistant chooses the tools',
        blocks: [
          {
            p: 'You type a question or request in Claude Desktop, just as you would ask a colleague. Claude decides which tools to use, runs them, and writes the answer from the results. You do not need to know the tool names, although naming a tool can help when you want a specific one (for example, “use calculate_isolation_and_exclusion”).',
          },
          {
            p: 'Claude often uses several tools in a row. A request such as “Build a community health profile for our town, map the most vulnerable census tracts, and put it all in a Word report on our letterhead” uses three tools. Claude Desktop may ask your permission before it runs a tool, so you can see what it is about to do.',
          },
        ],
      },
      {
        title: 'Your data folder',
        blocks: [
          {
            p: 'During installation you choose one folder on your computer that the server is allowed to use. This guide calls it **PHN-Data**. The server cannot see anything outside it.',
          },
          {
            table: {
              caption: 'Table 2. Folders the server uses',
              widths: [2400, 6960],
              header: ['Folder', 'What goes there'],
              rows: [
                ['PHN-Data', 'Files you want the assistant to be able to use: line lists, rosters, spreadsheets of counts, documents, a logo for your letterhead. You can make subfolders.'],
                ['PHN-Data/policies', 'Your agency’s policies and procedures, for the policy search tool.'],
                ['PHN-Data/output', 'Everything the tools create: documents, workbooks, de-identified files, roster results, and contact-tracing workbooks. Charts go in output/charts and maps in output/maps.'],
              ],
            },
          },
          {
            p: 'Refer to files by their name inside the data folder, for example “line_list.csv” or “rosters/kindergarten.xlsx”. If you are not sure of a name, ask “What files are in my data folder?”',
          },
        ],
      },
      {
        title: 'What every answer includes',
        blocks: [
          { p: 'Every tool returns the same kinds of information, which Claude uses in its reply:' },
          {
            bullets: [
              '**A summary** of the result.',
              '**Sources**, each with its publisher, date, link, and how it was obtained: *live* (from the internet just now), *cache* (a recent copy saved on your computer), *snapshot* (a dated copy bundled with the server), *local* (your own files), or *computed* (calculated by the server).',
              '**Caveats**: limits of the data or method that you should know about.',
              '**A privacy note**: whether any record-level data were returned, whether small numbers were suppressed, whether identifiers were detected, and which files were written to your output folder.',
              '**A decision-support notice** on clinical and legal answers, with the review status of the content used.',
            ],
          },
          { note: 'If Claude’s reply leaves out the sources or caveats, ask: “Show me the sources, dates, and caveats for that answer.”' },
        ],
      },
      {
        title: 'How your privacy is protected',
        blocks: [
          {
            bullets: [
              '**Sandbox.** Tools read only inside your data folder and write only to its output folder.',
              '**Aggregates first.** Tools that analyze your files return structure, counts, and statistics, never rows of data, unless you ask for sample rows. Even then, identifier columns are masked, dates are shortened to the year, ZIP codes to three digits, and ages over 89 are shown as 90+.',
              '**Identifier detection.** Column names, value patterns, and public name lists are used to flag the 18 HIPAA identifiers. Document text is redacted before it is returned.',
              '**Small-cell suppression.** Counts from 1 up to your threshold minus one (1 to 4 with the default threshold of 5) are hidden in frequency tables, along with any other cells needed to keep them from being recalculated from totals.',
              '**Local mail merge.** Letters and rosters are written to your output folder and never shown in the chat.',
              '**No telemetry.** The server contacts only the public data sources listed by the data source status tool, and it can run fully offline.',
            ],
          },
        ],
      },
      {
        title: 'Profiles and tool groups',
        blocks: [
          {
            p: 'Showing an AI assistant every tool at once can crowd its memory and make it less accurate, so the tools are organized into groups (called toolsets), and a **profile** turns on the groups that fit your role. The **core** group is always on. If no profile is set, every tool is available.',
          },
          {
            table: {
              caption: 'Table 3. Profiles and the tool groups they include in version ' + VERSION,
              widths: [1700, 2500, 2860, 2300],
              header: ['Profile', 'Intended for', 'Tool groups included', 'Not included'],
              rows: [
                ['school_nurse', 'School nurses', 'core, school, vax, cd, bh, comms, epi', 'mch, aging, epr, env, equity, chronic, ebp'],
                ['cd_nurse', 'Communicable disease and epidemiology nurses at a board of health', 'core, cd, vax, epi, comms, equity', 'school, mch, aging, bh, epr, env, chronic, ebp'],
                ['phn_leader', 'Public health nurse leaders and directors', 'core, epi, equity, comms, ebp, chronic', 'cd, vax, school, mch, aging, bh, epr, env'],
                ['preparedness', 'Emergency preparedness coordinators', 'core, epr, env, epi, equity, comms, cd', 'vax, school, mch, aging, bh, chronic, ebp'],
                ['(none)', 'Anyone who wants every tool', 'All groups', '—'],
              ],
            },
          },
          {
            p: 'Each tool description in this guide names its group. If a tool you need is not available, ask Claude to change your profile or turn on the group, for example “Turn on the ebp toolset.” If your profile was set in the Claude Desktop configuration file, that setting takes priority; the tool will say so, and you or IT can change the file. The phn_leader profile also switches on three groups reserved for future versions (planning, leadership, and policy); they have no tools yet. The mch and aging groups are not part of any profile: ask Claude to turn them on (for example “Turn on the mch toolset”), or use no profile.',
          },
        ],
      },
      {
        title: 'Where the information comes from',
        blocks: [
          {
            bullets: [
              '**Bundled reference content**, drafted from Massachusetts regulations and laws (105 CMR 300.000 and the school health, lead, and child care regulations cited in each chapter; the Massachusetts General Laws on mandated reporting and health care proxies), DPH’s disease guide and guidance, DPH’s school immunization requirements, CDC and ACIP, AAP, ACOG, AAFP, Immunize.org, EPA, CMS, and FEMA’s incident command and exercise materials, each entry with its citation and a confidence level. All of it is pending expert review.',
              '**Dated data snapshots** bundled with the server: American Community Survey estimates, the Social Vulnerability Index, CDC PLACES, map boundaries, DPH’s school immunization survey, CDC immunization coverage estimates, an offline copy of DPH’s disease guide and fact sheets, recent CDC surveillance data, CDC and WHO growth charts, DPH lead screening, birth, overdose, and youth survey tables, directories of aging and behavioral health services, CMS nursing home data, FEMA disaster declarations, and HHS emPOWER counts.',
              '**Live public data**, when the internet is available: the Census Bureau, CDC’s data portal (data.cdc.gov), PubMed, CDC’s MMWR and Health Alert Network, RxNorm, openFDA, DailyMed, the National Weather Service, AirNow, OpenFEMA, HHS emPOWER, CMS Provider Data, HRSA, and FindTreatment.gov.',
              '**Your own files** in the data folder.',
            ],
          },
          { p: 'To see the date and review status of every source, ask “What data sources do you use, and how current are they?” (section 4.2).' },
        ],
      },
    ],
  },
  {
    title: 'Getting started',
    intro: [],
    sections: [
      {
        title: 'Before you begin',
        blocks: [
          {
            p: 'The server runs inside a “container,” a sealed package that holds the program and its data, on your Windows or Mac computer. Installation is done once and usually needs your IT department, because agencies often control which software can be installed. In outline:',
          },
          {
            steps: [
              'Ask IT which container program is approved (Docker Desktop, Podman Desktop, or Rancher Desktop) and whether your network needs a proxy or a special security certificate. The project includes a sample request you can send to IT (docs/acceptance/phase-0-1.md).',
              'Install the container program and download the server.',
              'Create your data folder (for example PHN-Data in your home folder) with a **policies** subfolder. The **output** subfolder is created automatically.',
              'Add the server to Claude Desktop’s configuration file, as shown in the project’s README file, choosing your profile there if you like.',
              'Restart Claude Desktop. The server’s tools then appear in Claude Desktop’s list of tools.',
            ],
          },
        ],
      },
      {
        title: 'Your first five minutes',
        blocks: [
          { p: 'Try these requests, in order:' },
          {
            steps: [
              'Save your town and board of health phone number: “Set my jurisdiction to Amherst, and my board of health phone to 413-555-0100.”',
              'Add your letterhead: “Set my letterhead to Anytown Health Department, 1 Main Street, Anytown, MA, phone 508-555-0100, with the logo in logo.png.”',
              'See what the server knows and how current it is: “What data sources do you use, and how current are they?”',
              'Ask a real question: “Is a positive HBsAg reportable, and how fast?”',
            ],
          },
        ],
      },
      {
        title: 'Tips for good results',
        blocks: [
          {
            bullets: [
              '**Be specific.** Give the disease, the setting (school, child care, food handler, health care worker), whether the person is a case or a contact, and the relevant dates.',
              '**Give exact dates.** Tools count days from the dates you give. “Onset October 20, 2026” is better than “about two weeks ago.” In your files, a two-digit year is read in this century unless that would be more than a year in the future (3/7/24 is 2024; a birth date of 5/2/50 is 1950). Dates before 1900 or after 2100 are treated as errors.',
              '**Point to files by name.** “Analyze outbreak/potluck.xlsx” lets the tool read the file locally, so identifiers never pass through the chat.',
              '**Never type identifiers.** Leave names, dates of birth with names, addresses, and record numbers out of your messages.',
              '**Ask for the method and sources.** “How did you calculate that?” and “What are the caveats?” are always good follow-up questions.',
              '**Ask for the format you need.** “Save that as a Word document on our letterhead,” “Make a chart,” or “Put the table in Excel.”',
              '**Check the review status.** Answers built on draft content say so. Confirm anything important against the cited source.',
            ],
          },
        ],
      },
    ],
  },
];

// ---------------------------------------------------------------------------------------
// Tool chapters: chapters 4–16. One entry per implemented tool.
// ---------------------------------------------------------------------------------------

const toolChapters = [
  {
    title: 'Setup and data sources',
    intro: 'These two tools set up the server for your agency and tell you how current its information is. Both are in the core group, which is always on.',
    tools: [
      {
        id: 'configure_jurisdiction',
        title: 'Set your jurisdiction and preferences',
        toolset: 'core',
        internet: 'no',
        files: 'no (settings are saved for later sessions)',
        does: 'Saves your settings so the other tools can use them: your city or town (the county and HMCC region are filled in for you), school district, board of health phone number, small-cell suppression threshold, preferred languages, profile or tool groups, letterhead, and which body’s vaccine recommendations to list first. Settings are kept between sessions.',
        when: [
          'The first time you use the server.',
          'Before creating documents, so they carry your agency’s letterhead.',
          'When a tool you need is not available and you want to change your profile or turn on a tool group.',
          'When your agency’s data release policy uses a suppression threshold other than 5.',
          'When you want immunization answers to list a different recommending body first.',
        ],
        provide: [
          'Your city or town (any of the 351; misspellings get suggestions) and, if you like, your school district (name or 8-digit DESE code).',
          'Your board of health phone number, shown in reportable-condition answers.',
          'A suppression threshold from 2 to 30 (default 5).',
          'Preferred languages, most important first.',
          'Letterhead: agency name (required), department, address, phone, fax, email, website, footer text, and a logo file (PNG or JPEG) in your data folder.',
          'Vaccine recommendation basis: DPH first, then the professional societies, then CDC (the default); CDC/ACIP; professional societies; or show all equally.',
          'A profile (school_nurse, cd_nurse, phn_leader, preparedness, or none) or a list of tool groups.',
        ],
        examples: [
          'Set my jurisdiction to Amherst, and my board of health phone to 413-555-0100.',
          'Our data release policy suppresses counts under 11. Set the suppression threshold to 11.',
          'Set my letterhead to Anytown Health Department, 1 Main Street, Anytown, MA, phone 508-555-0100, with the logo in logo.png.',
          'Show me my current settings.',
          'Switch my profile to cd_nurse.',
        ],
        returns: [
          'Your current settings and what changed, including the county and HMCC region found for your town.',
          'A note for any setting that is overridden by the Claude Desktop configuration file.',
        ],
        notes: [
          'Settings in the Claude Desktop configuration file take priority over saved settings.',
          'HMCC regions come from DPH’s 2005 preparedness-region map, corrected where DPH’s 2019 HMCC map differs (Wilmington is Region 3). You can set the region yourself if it is wrong.',
          'Changing your profile updates the tool list right away in clients that support it; otherwise restart Claude Desktop.',
          'To start over, ask Claude to reset all settings.',
          'If your profile or tool groups are set in the Claude Desktop configuration file (as in the README’s examples), they cannot be changed here: a profile or tool group you save is kept but has no effect, and the answer says so. A tool outside your groups then reports that the configuration file must be edited.',
        ],
      },
      {
        id: 'get_data_source_status',
        title: 'Check data sources and how current they are',
        toolset: 'core',
        internet: 'optional',
        files: 'no',
        does: 'Lists every source of information the server uses: the bundled reference content with its review status and review due date, the dated data snapshots with the source’s data year and build date, your own files, and the public websites the server can contact, with caveats for each. It can also test whether each website can be reached from your network right now.',
        when: [
          'Before relying on an answer for an important decision, to check how current the data are and whether the content has been reviewed.',
          'When writing a report that must state data years and sources.',
          'When something seems out of date, or when IT asks which websites the server contacts.',
          'To test whether your agency network lets the server reach CDC, Census, and other sources.',
        ],
        provide: [
          'Nothing is required.',
          'Optionally: ask it to test live connections, to report one source, or to include sources planned for later versions.',
        ],
        examples: [
          'What data sources do you use, and how current are they?',
          'Check whether you can reach the live CDC and Census data from this network.',
          'Has the reportable conditions list been reviewed yet?',
        ],
        returns: [
          'A list of sources with dates, review status, whether a newer cached copy is in use, whether each website is reachable, and caveats.',
        ],
        notes: [
          'In offline mode every website is shown as unreachable and nothing is contacted.',
          'All bundled reference content in version ' + VERSION + ' is pending subject-matter review.',
        ],
      },
    ],
  },
  {
    title: 'Your files and documents',
    intro: 'These tools work with the files in your data folder and produce finished documents. They are the foundation of the server’s privacy protections: they let the assistant use your data while keeping identifiers on your computer.',
    tools: [
      {
        id: 'read_local_file',
        title: 'Read files in your data folder',
        toolset: 'core',
        internet: 'no',
        files: 'no',
        does: 'Lists the files in your data folder and pulls text and tables out of PDF, Word, Excel, CSV, HTML, Markdown, and plain-text files so the assistant can summarize or use them. Identifiers are flagged and redacted before anything reaches the AI.',
        when: [
          'To summarize a guidance memo, report, or set of meeting notes you saved.',
          'To see which files are in your folder.',
          'To see the layout of a spreadsheet (sheets, columns, number of rows) before analyzing it.',
        ],
        provide: [
          'The file or folder name.',
          'For spreadsheets: which sheet, and whether you want a few masked sample rows.',
        ],
        examples: [
          'What files are in my data folder?',
          'Read memos/pertussis_update.pdf and summarize what we need to do.',
          'What columns are in rosters/kindergarten.xlsx?',
        ],
        returns: [
          'For documents: the text, up to about 20,000 characters by default (up to 100,000 on request).',
          'For spreadsheets: sheet names, column names, row counts, and which columns may contain identifiers. Data rows are not shown unless you ask for samples.',
        ],
        notes: [
          'Sample rows, when requested, have identifier columns replaced by [REDACTED], dates shortened to the year, ZIP codes to three digits, and ages over 89 shown as 90+.',
          'In documents, Social Security numbers and labeled dates of birth and record, member, account, and license numbers are removed. If any are found, the whole document is redacted strictly (names, phone numbers, emails, addresses, and dates as well).',
          'Redaction cannot be turned off. Scanned PDFs without a text layer cannot be read (there is no text recognition).',
          'Headings and titles are redacted like the rest of the text. A document with a table or list of clients (names next to dates of birth or addresses) is redacted strictly.',
          'If a spreadsheet has no header row, its columns are shown as column_1, column_2, and so on, so that no names or values appear as column names.',
        ],
      },
      {
        id: 'search_agency_policies',
        title: 'Search your agency’s policies',
        toolset: 'core',
        internet: 'no',
        files: 'no',
        does: 'Answers “What does our policy say?” by searching the policies and procedures in your policies folder and returning the matching passages with citations to the file and section or page.',
        when: [
          'A staff member asks how your agency handles something.',
          'Before acting on an exposure, cold-chain problem, or other event your policies cover.',
          'To compare your policy with new DPH guidance (together with the DPH guide search in section 6.2).',
        ],
        provide: [
          'Words or phrases to look for. You can use “quoted phrases” and AND, OR, and NOT.',
          'Optionally, how many passages you want (5 by default, up to 20).',
        ],
        examples: [
          'What does our policy say about needlestick injuries?',
          'Search our policies for “cold chain” AND “power outage”.',
          'Who reads tuberculin skin test results under our TB policy?',
        ],
        returns: [
          'Matching passages with your search words in bold and citations such as “Exposure Control Plan.docx, § Reporting” or “TB policy.pdf, p. 4”.',
        ],
        notes: [
          'Works with PDF, Word, Markdown, text, and HTML files. The search index updates itself when files change, so the first search after adding files can be slower.',
          'Scanned PDFs without a text layer cannot be searched.',
          'Headings in the passages are redacted too. A file that holds client information (a date of birth or record number) is redacted strictly.',
        ],
      },
      {
        id: 'export_document',
        title: 'Create accessible documents and mail-merged letters',
        toolset: 'core',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Turns text the assistant drafts into finished files (Word, tagged PDF, Excel, CSV, SVG, or PNG) with your letterhead and page numbers, and checks every Word and PDF file for accessibility before handing it over. It also mail-merges letters from a spreadsheet on your computer.',
        when: [
          'A fact sheet, letter, memo, report, or handout is ready to save.',
          'Letters to many families from a roster, such as exclusion or reminder letters.',
          'A table you want in Excel or CSV.',
        ],
        provide: [
          'What the document should contain (Claude drafts it), a title, and the formats you want.',
          'Optionally: a file name, the document language (for example Spanish), and whether to use the letterhead.',
          'For a mail merge: the spreadsheet in your data folder, and whether you want one combined file or one file per person. Claude writes the letter with placeholders such as {{Student Name}} that match the spreadsheet’s column names.',
        ],
        examples: [
          'Turn that summary into a Word document on our letterhead.',
          'Make a one-page pertussis fact sheet for parents at a 6th-grade reading level and save it as Word and PDF.',
          'Draft an exclusion letter with placeholders for the student’s name and grade, then mail-merge it with exclusions.xlsx, one file per student.',
        ],
        returns: [
          'The names of the files saved in your output folder and the accessibility check results.',
          'For a mail merge: only counts, field names, and file names. The merged letters themselves are never shown in the chat.',
        ],
        notes: [
          'Existing files are never overwritten; a number is added to the new file’s name.',
          'Every image needs alternative text.',
          'PDF output is single-column, and letters outside the bundled font (some non-Latin scripts) can show as “?”. Use Word for those languages.',
          'Setting a document’s language does not translate it. Have translated materials checked by a qualified translator before you distribute them.',
          'A line of three dashes (---) does not start a new page; use \\pagebreak. Lists inside lists stay under their parent item.',
          'In CSV files, a cell that starts with =, +, -, or @ and is not a number gets a leading apostrophe, so a spreadsheet does not run it as a formula.',
        ],
      },
      {
        id: 'check_document_accessibility',
        title: 'Check a document for accessibility',
        toolset: 'comms',
        internet: 'no',
        files: 'no',
        does: 'Checks a Word, HTML, or PDF file against Section 508 and WCAG 2.1 AA and explains how to fix each problem in Word or Acrobat.',
        when: [
          'Before posting a document on the town website or sending it to families.',
          'When you receive materials from a partner and want to reuse them.',
          'After editing a document the server created.',
        ],
        provide: ['The file name in your data folder.'],
        examples: [
          'Check flu_clinic_flyer.docx for accessibility.',
          'Is output/annual_report.pdf accessible? Tell me how to fix any problems.',
        ],
        returns: [
          'Each check with its WCAG success criterion and a pass, warning, or fail: document title, language, headings and their order, bold text posing as headings, alternative text, table header rows and merged cells, color contrast, link text, empty paragraphs and typed bullets, and, for PDFs, tagging, scanned pages, and embedded fonts.',
          'How to fix each failure, and the checks only a person can do (reading order, use of color, and the quality of alternative text).',
        ],
        notes: [
          'Automated checks cannot confirm everything; do the manual checks it lists.',
          'This tool is in the comms group, which every profile includes.',
          'For PDFs, headings, table headers, color contrast, and link text are listed as manual checks; check the Word or HTML original with this tool, or use Acrobat’s checker.',
          'Text quoted in the report (titles, headings, first table rows) has possible names, phone numbers, and other identifiers replaced with [REDACTED].',
        ],
      },
      {
        id: 'profile_local_dataset',
        title: 'Describe a dataset without exposing it',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Describes a spreadsheet or CSV file in your data folder: the number of rows and columns, duplicates, the type of data in each column, missing values, distinct values, and which columns look like identifiers (with the evidence, never the values).',
        when: [
          'Your first look at a line list or a data export before analyzing it.',
          'Checking data quality, such as missing onset dates or duplicate records.',
          'Finding out whether a file contains identifiers before you share it.',
        ],
        provide: [
          'The file name (and sheet).',
          'Optionally, ask for value summaries: averages and spreads for numbers, date ranges, and frequency tables for categories.',
        ],
        examples: [
          'Profile line_list.csv and tell me if it contains identifiers.',
          'How complete is the onset date column in outbreak/norovirus_ll.xlsx?',
          'Profile vaccination_export.csv with value summaries.',
        ],
        returns: [
          'A column-by-column description.',
          'With value summaries: mean, standard deviation, quartiles, and outlier counts for numeric columns; month ranges for dates (not birth dates); and frequency tables, with small-cell suppression, for categories, towns, and ZIP codes.',
        ],
        notes: [
          'Identifier columns never show values. Minimum and maximum values are shown only when there are enough records, and ages over 89 appear as 90+.',
          'A number column with fewer values than your suppression threshold shows no statistics, and the smallest and largest values are hidden when almost every value is different (as with ID numbers).',
        ],
      },
      {
        id: 'deidentify_dataset',
        title: 'Make a de-identified copy of a dataset',
        toolset: 'epi',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Writes a de-identified copy of a dataset using the HIPAA Safe Harbor method (or a limited data set), with a report of exactly what was changed.',
        when: [
          'Before sharing data with a school, a partner, a student, or a researcher.',
          'When preparing data for someone else to analyze.',
        ],
        provide: [
          'The file name (and sheet).',
          'The method: Safe Harbor (the default) or limited data set.',
          'Optionally: whether dates of birth become age (the default) or year; columns to drop; columns to keep unchanged; whether to keep free-text columns after strict redaction (they are dropped by default); and CSV or Excel output.',
        ],
        examples: [
          'Make a de-identified copy of line_list.csv I can share with the school.',
          'De-identify clinic_visits.xlsx as a limited data set and save it as Excel.',
        ],
        returns: [
          'A report of what was dropped, generalized, or redacted, and how many records have a rare combination of characteristics (shared by fewer records than your suppression threshold) that could make them identifiable.',
          'The de-identified file and the report are saved in your output folder; only the report is shown in the chat.',
        ],
        notes: [
          'Safe Harbor drops names, addresses, town and county, phone and fax numbers, email, Social Security numbers, and record, plan, account, license, vehicle, and device numbers, web and IP addresses, photographs, and biometrics. It shortens dates to the year, replaces dates of birth with age (90+ for 90 and older), and shortens ZIP codes to three digits (000 for the low-population prefixes in HHS guidance).',
          'ID numbers are replaced with new codes and no link back to the originals is kept, so keep your original file.',
          'If you keep any column unchanged, the output is marked NOT de-identified.',
          'A limited data set is still protected health information; under HIPAA it is shared only under a data use agreement. Follow your agency’s policy.',
          'Safe Harbor also removes full dates and ZIP codes written inside text cells (for example “onset 3/7/2024” in a notes column). A limited data set drops latitude and longitude columns.',
        ],
      },
      {
        id: 'apply_small_cell_suppression',
        title: 'Suppress small numbers in a table',
        toolset: 'epi',
        internet: 'no',
        files: 'yes, for tables read from a file',
        does: 'Applies DPH-style small-cell suppression to a table of counts. It hides counts from 1 up to your threshold minus one, then hides additional cells (complementary suppression) so the hidden numbers cannot be worked out from row and column totals.',
        when: [
          'Before publishing or sharing any table of counts: an annual report, a website, or a board of health presentation.',
          'Counts by school, town, age group, or any small population.',
        ],
        provide: [
          'The table: type or paste the counts, or name a CSV or Excel file of counts.',
          'Optionally: the threshold (your saved setting by default), whether to suppress zeros, and the marker (* by default).',
        ],
        examples: [
          'Apply small-cell suppression to cases_by_town_and_age.xlsx.',
          'Suppress this table before I put it in the board of health report: [paste the counts]',
        ],
        returns: [
          'The suppressed table and which cells were hidden and why.',
          'For files, a copy named <name>_suppressed.csv with a footnote, in your output folder.',
        ],
        notes: [
          'Tables that contain direct identifiers are refused, because they are record-level data, not a table of counts.',
          'Totals are hidden only as a last resort. The same marker is used for primary and complementary suppression so readers cannot tell them apart.',
          'A first column of numbers that are labels (grade, age, year, week, ZIP code) is treated as a label, not a count. Label cells that look like names, phone numbers, or long notes are refused.',
        ],
      },
    ],
  },
  {
    title: 'Communicable disease',
    intro: 'These tools support case and contact management, outbreak investigation, and surveillance. All are in the cd group, which the school_nurse, cd_nurse, and preparedness profiles include. The disease rules, measles guidance, infection control content, and DPH guide corpus they use are pending review by the subject matter expert, and every answer says so. Section 6.12 describes a guided outbreak investigation that uses these tools in order.',
    tools: [
      {
        id: 'lookup_reportable_condition',
        title: 'Look up a reportable condition',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'Tells you whether a disease, organism, or laboratory result is reportable in Massachusetts under 105 CMR 300.000, how fast (immediately by phone, within 24 hours, or through a program-specific route), who reports, to whom, and how, with the regulation sections and the matching chapter of DPH’s *Guide to Surveillance, Reporting and Control*.',
        when: [
          'A provider, laboratory, school, or resident asks whether something is reportable.',
          'You receive a lab result and need to know the reporting timeframe.',
          'Orienting new staff to reporting requirements.',
        ],
        provide: ['The disease, organism, or lab result in your own words, such as “whooping cough”, “Salmonella Typhi”, or “positive HBsAg”.'],
        examples: [
          'Is a positive HBsAg reportable, and how fast?',
          'Who reports a case of Lyme disease in Massachusetts, and to whom?',
          'A child care director called about a child with E. coli O157. What has to be reported, and how fast?',
        ],
        returns: [
          'Whether the condition is reportable, the timeframe, who reports, to whom, and how.',
          'The regulation sections, the DPH guide chapter when there is one, your board of health phone number (if saved), and the DPH Epidemiology line.',
          'The confidence level of the entry, with an extra caveat for lower-confidence entries.',
        ],
        notes: [
          'If nothing matches, that does **not** mean the condition is not reportable: clusters and unusual illnesses always are.',
          'The list was re-checked against the regulation text and DPH’s current reporting lists in November 2026 and is pending subject-matter review.',
          'It does not guess. If your words fit more than one condition (“hepatitis”) or are too general (“fever”, “stomach flu”), the answer lists the possible conditions for you to choose from. Ask again with the specific name.',
        ],
      },
      {
        id: 'search_ma_disease_control_guide',
        title: 'Search the DPH disease control guide',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'Searches an offline copy of 25 chapters of DPH’s *Guide to Surveillance, Reporting and Control* and 66 DPH fact sheets and guidance pages, and quotes the matching passages word for word with the section, the document’s date, and its web address.',
        when: [
          'You need DPH’s case management or control measures for a disease.',
          'You want to quote DPH’s exact wording in a letter or guidance to a school.',
          'You are looking for a DPH fact sheet to give to families.',
        ],
        provide: [
          'Words or phrases to search for, and optionally a disease (common names work, such as “whooping cough”).',
          'Optionally: the kind of document (guide chapter, fact sheet, or guidance), whether you want whole sections, or a list of every document.',
        ],
        examples: [
          'What does the DPH guide say about excluding food handlers with hepatitis A?',
          'Show me the outline of the DPH pertussis chapter.',
          'Find DPH’s fact sheet on scabies.',
          'List the documents you can search.',
        ],
        returns: [
          'Verbatim passages with the section path (for example “Section 4: Controlling Further Spread › A. Isolation and Quarantine Requirements”), the document’s date, and its link.',
        ],
        notes: [
          'Many guide chapters date from 2016 to 2018. Check the date shown and compare with DPH’s current guidance.',
          'Tables are flattened and flowcharts are left out. The *Streptococcus pneumoniae* chapter is not included.',
        ],
      },
      {
        id: 'calculate_isolation_and_exclusion',
        title: 'Calculate isolation, quarantine, and exclusion dates',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'Works out isolation, quarantine, and exclusion periods for a case or a contact, for each setting (child care, school, food handler, health care worker, and other), from 105 CMR 300.200 and DPH guidance. When you give dates, it calculates the actual calendar dates. It quotes the regulation’s wording and cites each rule.',
        when: [
          'Deciding when a child can return to school or child care.',
          'Deciding whether a food handler or health care worker can return to work.',
          'Working out exclusion periods for susceptible contacts.',
          'Answering a school nurse’s or child care director’s question.',
        ],
        provide: [
          'The disease, whether the person is a case or a contact, and the settings that matter.',
          'Dates: symptom onset, positive specimen, first and last exposure, the index case’s onset, treatment start, and symptom resolution, as they apply.',
          'Facts that change the rules: symptomatic or not, susceptible or immune, received prophylaxis or immune globulin, part of an outbreak, continuous exposure, lesions present, diarrhea.',
          'Optionally, a date to check whether the person is still excluded on that day.',
        ],
        examples: [
          'A second grader with pertussis started antibiotics on November 3. When can the student return to school?',
          'A susceptible child care worker was exposed to measles on October 28 and October 30. What are the exclusion dates?',
          'A food handler has salmonellosis and the diarrhea has stopped. What is needed before returning to work, and is it different during an outbreak?',
        ],
        returns: [
          'For each setting: the rule, the start and end dates (or the conditions to be met, such as negative stool specimens), the regulation’s wording, the citation, and a confidence level.',
          'Rules that depend on facts you did not give are shown as “only if …”.',
        ],
        notes: [
          'Day counting: day 0 is the date the rule counts from, and “through day 21” means the person may return on day 22.',
          'Rules cover 37 diseases. Tuberculosis dates are not calculated: the tool shows the regulation’s clearance criteria for reference and refers you to the clinician and DPH’s TB program.',
          'Decision support, pending review.',
          'The disease must be named specifically. A general or ambiguous name (“hepatitis”, “gastroenteritis”) gets a list of the diseases it could mean.',
        ],
      },
      {
        id: 'assess_measles_exposure',
        title: 'Assess a measles exposure',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'For a person exposed to measles: evaluates evidence of immunity, calculates the deadlines for post-exposure MMR and immune globulin and the immune globulin dose, gives exclusion dates, identifies priority groups, and gives the end date of symptom monitoring.',
        when: [
          'After a measles exposure at a clinic, school, emergency department, or other public place.',
          'Triaging a list of exposed people: who needs post-exposure prophylaxis, and by when.',
        ],
        provide: [
          'The first and last exposure dates (at least one), and the case’s rash onset date if known (the tool checks the exposure fell within the infectious period).',
          'The setting or immunity group: general, school, college, or travel, or health care personnel.',
          'Evidence of immunity: birth year or age in months, documented MMR doses, positive measles IgG, or laboratory-confirmed measles.',
          'Pregnancy, severe immunocompromise, weight (for the immune globulin dose), and whether immune globulin was already given.',
        ],
        examples: [
          'A 9-month-old weighing 8 kg was in a waiting room with a measles case on November 2. The case’s rash began November 4. What should we do, and by when?',
          'A nurse born in 1975 with one documented MMR dose was exposed on October 30. Does the nurse have evidence of immunity?',
        ],
        returns: [
          'The immunity assessment with its reasoning.',
          'The MMR and immune globulin deadlines, whether each window is still open, and the immune globulin dose.',
          'Exclusion dates, priority groups, the end of symptom monitoring, and DPH program notes.',
        ],
        notes: [
          'Based on DPH’s 2022 measles chapter and the 2013 ACIP statement; pending review.',
          'Suspected measles must be reported promptly (check the timeframe with the tool in section 6.1); coordinate exposure follow-up with DPH Epidemiology.',
          'A baby under 12 months has no evidence of immunity whatever doses are reported, because doses before the first birthday do not count.',
          'If you give the case’s rash onset and the exposure dates go beyond the case’s infectious period, the deadlines are counted from the days inside that period, and the answer says so.',
        ],
      },
      {
        id: 'plan_contact_tracing',
        title: 'Plan contact tracing',
        toolset: 'cd',
        internet: 'no',
        files: 'yes, a contact-tracking workbook in your output folder',
        does: 'For a case of a communicable disease, works out the infectious period, the window for identifying contacts, the contact definition, testing and monitoring guidance, and the date rules for each contact. It saves a blank Excel contact-tracking workbook that calculates each contact’s dates for you.',
        when: [
          'Starting a case investigation that involves contacts, such as pertussis, measles, invasive meningococcal disease, or hepatitis A.',
          'Organizing contact follow-up shared among several staff.',
        ],
        provide: [
          'The disease and the index case’s dates: symptom onset, positive specimen, and treatment start, as known.',
          'Optionally, the date isolation began (it ends the contact window early) and how many rows you want in the workbook (50 by default).',
        ],
        examples: [
          'Plan contact tracing for a pertussis case with cough onset October 20 who started azithromycin on November 1.',
          'Set up a contact-tracking workbook for a hepatitis A case with symptom onset November 5.',
        ],
        returns: [
          'The infectious period, the contact window, the contact definition, testing and monitoring guidance, prophylaxis deadlines, and exclusion rules.',
          'A workbook named contact_tracking_<disease>.xlsx. Enter each contact’s first and last exposure dates and it fills in when symptoms could start, when monitoring ends, prophylaxis deadlines, and exclusion dates. A second sheet lists the rules and sources.',
        ],
        notes: [
          'Enter contacts’ names in the workbook on your computer, not in the chat.',
          'Decision support, pending review.',
          'Each exclusion column in the workbook says who it applies to (for example only health care workers without symptoms or prophylaxis for pertussis), so check the column heading before excluding a contact.',
        ],
      },
      {
        id: 'infection_control_for_congregate_settings',
        title: 'Infection control for congregate settings',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'For a pathogen and a setting (long-term care, school, child care, shelter, or day program), gives the transmission-based precautions and how long to keep them, what each precaution means and the personal protective equipment it needs, notes on cohorting and hand hygiene, EPA lists of registered disinfectants with claims against the pathogen, and DPH notes for the setting.',
        when: [
          'Advising a nursing home during a norovirus outbreak.',
          'A scabies case in a shelter, or *C. difficile* in a day program.',
          'Advising a school on cleaning after a gastrointestinal outbreak, or helping a facility choose a disinfectant.',
        ],
        provide: [
          'The pathogen, by name or common alias (“norovirus”, “C. diff”, “flu”, “scabies”, “MRSA”).',
          'The setting, or leave it out to see all five.',
        ],
        examples: [
          'There is a norovirus outbreak at a long-term care facility. What precautions, PPE, and disinfectants should they use?',
          'What should a homeless shelter do about a case of scabies?',
          'Which EPA-registered disinfectants work against C. diff?',
        ],
        returns: [
          'Precautions and their duration (CDC/HICPAC isolation guideline, Appendix A), what each means and its PPE (105 CMR 300.020), cohorting and hand hygiene notes, links to the EPA disinfectant lists, and DPH notes for the setting.',
        ],
        notes: [
          'Decision support, pending review. The facility’s own infection prevention program and current DPH guidance also apply.',
          'Pathogens that are not in the server’s content (for example Ebola or Legionella) get a message saying so, rather than guidance for a different organism.',
        ],
      },
      {
        id: 'analyze_outbreak_line_list',
        title: 'Analyze an outbreak line list',
        toolset: 'cd',
        internet: 'no',
        files: 'no',
        does: 'Analyzes a line list of ill and well people (CSV or Excel) in your data folder. It counts cases, describes them by person, place, and time, calculates attack rates for each food or exposure with risk ratios and p-values, points out likely vehicles, and estimates the probable exposure window from the incubation period. Only aggregate results go to the AI.',
        when: [
          'A foodborne outbreak after a meal or event.',
          'A gastrointestinal or respiratory cluster in a facility.',
          'Testing hypotheses about which food or exposure caused illness.',
        ],
        provide: [
          'The file (and sheet). Columns for onset date and time, case status, age, sex, town, and exposures are found by name; tell Claude if the names are unusual.',
          'Which values count as cases (by default confirmed and probable, or “yes”) and any case-definition filters, such as required symptoms or an onset date range.',
          'The suspected disease or its incubation period, and the date and time of a known meal or event.',
          'Whether the study is a cohort (everyone at the event, the default) or a case-control study.',
        ],
        examples: [
          'Analyze outbreak/potluck_line_list.xlsx. The potluck was October 12 at 6 p.m. and we suspect norovirus.',
          'Count only confirmed cases with vomiting or diarrhea and recalculate the attack rates.',
          'This was a case-control study, not a cohort. Run it again.',
        ],
        returns: [
          'Numbers of cases, non-cases, and records excluded by the case definition.',
          'Tables by age group, sex, town, onset date, and symptom, with small cells suppressed.',
          'Attack rates for each exposure with risk ratios (or odds ratios for a case-control study) and Fisher exact p-values; likely vehicles (p < 0.05 with a lower confidence limit above 1); and the probable exposure window.',
        ],
        notes: [
          'Identifier columns are never used, and no row-level values are returned.',
          'In the exposure table, small counts are shown as <5, but the ratios are calculated from all records, so they could reveal a small count; the answer says so. Check this against your data release policy before sharing.',
          'A statistical association is not proof. Interpret it with the laboratory and environmental findings.',
          'With fewer cases with onset dates than your suppression threshold, the median onset and the probable exposure window are not shown, because they would reveal individual onset times.',
          'Foods or exposures where no one was recorded as exposed, or no one as not exposed, are listed in the caveats. If a blank cell means “did not eat”, enter N and run the analysis again.',
          'If the agent is not known yet, you can still analyze the line list: leave the disease out (or describe it, such as “GI illness, agent unknown”), and give incubation hours if you want an exposure window.',
        ],
      },
      {
        id: 'generate_epi_curve',
        title: 'Draw an epidemic curve',
        toolset: 'cd',
        internet: 'no',
        files: 'yes, a chart in output/charts',
        does: 'Draws an epidemic curve from a line list or a list of onset counts, with time intervals sized to the disease’s incubation period, optional color groups, and markers for a known exposure, and describes the likely pattern of spread.',
        when: [
          'During any outbreak, to see its shape and timing.',
          'For board of health, facility, or DPH briefings and reports.',
        ],
        provide: [
          'A line list file, or onset counts by date (or date and time).',
          'The suspected disease or its incubation period, and optionally a column to color by (up to 8 groups), the interval length, the date and time of a known exposure, and a title.',
        ],
        examples: [
          'Make an epi curve from outbreak/potluck_line_list.xlsx, colored by case status, with the October 12 6 p.m. dinner marked.',
          'Draw an epi curve from these daily onset counts: [paste the counts]',
          'Redo it with 12-hour intervals.',
        ],
        returns: [
          'The chart (PNG and SVG) with alternative text and a data table, saved in output/charts.',
          'Counts per interval (small counts suppressed in the chat) and the likely pattern (point source, continuous common source, intermittent common source, or propagated) with its reasoning and uncertainty.',
        ],
        notes: [
          'The saved chart shows exact counts. Review it, and suppress small numbers if needed, before sharing it outside your agency.',
          'The pattern is a hypothesis to test, not a conclusion.',
          'Dates must be between 1900 and 2100; a two-digit year such as 5/3/26 is read as 2026.',
        ],
      },
      {
        id: 'detect_aberrations',
        title: 'Detect unusual increases',
        toolset: 'cd',
        internet: 'no',
        files: 'only if you ask for a chart',
        does: 'Checks a series of daily, weekly, or monthly counts for unusual increases using CDC’s EARS methods (C1, C2, and C3), CUSUM, and the Farrington method, and explains the strengths and limits of each.',
        when: [
          'Is this week’s count of gastrointestinal illness, absences, or emergency visits higher than expected?',
          'Confirming that an outbreak exists (an early step in any investigation).',
          'Routine monitoring of reportable diseases or syndromes.',
        ],
        provide: [
          'The counts, oldest first, for at least 8 periods, and whether they are daily, weekly, or monthly.',
          'Optionally: a start date or labels for the periods, which methods to use, how many recent periods to report (12 by default), and whether to draw a chart.',
        ],
        examples: [
          'Here are weekly campylobacteriosis counts for the last three years, oldest first: [paste the counts]. Is the latest week unusual?',
          'Check these daily school absence counts since September 2 for an increase: [paste the counts]',
        ],
        returns: [
          'Signals by method, thresholds for the most recent periods, a plain-language interpretation, and the trade-offs between methods.',
        ],
        notes: [
          'A signal means “look into this,” not “there is an outbreak.”',
          'The Farrington method needs at least a year of history and is not used for daily counts.',
          'Type only counts, never line-level data.',
        ],
      },
      {
        id: 'get_notifiable_disease_trends',
        title: 'Follow national notifiable disease trends',
        toolset: 'cd',
        internet: 'optional',
        files: 'no',
        does: 'Shows CDC’s weekly National Notifiable Diseases Surveillance System (NNDSS) counts for Massachusetts, New England, and the United States, compared with the same point in each of the five previous years. Without a condition, it gives an overview of every condition, with those above their five-year range first.',
        when: [
          'Is pertussis (or another condition) higher than usual in Massachusetts this year?',
          'Context for a local case or a board of health update.',
          'Weekly situational awareness.',
        ],
        provide: [
          'A condition (or none for the overview).',
          'Optionally: up to 6 reporting areas (states, regions, or the United States), an MMWR year and week, and how many overview rows to show.',
        ],
        examples: [
          'How does pertussis in Massachusetts this year compare with the last five years?',
          'Which notifiable diseases in Massachusetts are above their five-year range?',
          'Compare measles cases this year in Massachusetts, New England, and the U.S.',
        ],
        returns: [
          'For each area: this week’s count; the year-to-date count and the last 4 weeks, each with the five-year baseline (mean, minimum, maximum, and ratio); and provisional year-end totals for the previous five years.',
          'Weekly counts for the last 8 weeks for the first area, and an explanation of the method.',
        ],
        notes: [
          'Counts are provisional, and DPH’s MAVEN counts can differ.',
          'Conditions not in the weekly tables (for example Lyme disease) are not available; the tool points you to CDC’s annual tables.',
          'Offline, only the most recent week in the bundled snapshot is available.',
          'In a year with an MMWR week 53, that week is compared with week 52 of earlier years.',
        ],
      },
      {
        id: 'get_respiratory_and_wastewater_surveillance',
        title: 'Respiratory virus and wastewater briefing',
        toolset: 'cd',
        internet: 'optional',
        files: 'no',
        does: 'Gives a weekly briefing on COVID-19, influenza, and RSV in Massachusetts from CDC data: hospital admissions, emergency department visits, test positivity, the acute respiratory illness activity level, and wastewater viral activity at each Massachusetts sampling site, each with its data date and trend.',
        when: [
          'Weekly situational awareness during respiratory season.',
          'Updates for the board of health, school leaders, or long-term care partners.',
          'Timing vaccination clinics and prevention messages.',
        ],
        provide: [
          'Optionally: which viruses (all three by default) and how many weeks of history (4 by default, 2 to 12).',
        ],
        examples: [
          'Give me this week’s respiratory virus briefing for Massachusetts.',
          'How has RSV been trending over the last 8 weeks?',
          'What are the wastewater levels at Massachusetts sampling sites?',
        ],
        returns: [
          'For each measure, the latest week compared with the week before: up or down when the change is at least 10%, otherwise stable (the server’s rule), plus CDC’s own categories as published.',
          'Wastewater sites by CDC activity category with the median level, and a table showing how current each source is.',
        ],
        notes: [
          'Geography differs by measure: test positivity is for New England or the nation, not Massachusetts alone.',
          'DPH’s respiratory illness dashboard is the Massachusetts source of record and is linked in every answer.',
          'MWRA’s Deer Island wastewater testing by Biobot ended in July 2026; its final values are shown.',
        ],
      },
    ],
    extras: [
      {
        title: 'Guided workflow: investigate an outbreak',
        blocks: [
          {
            p: 'Besides its tools, the server offers a ready-made **prompt** called “Investigate an outbreak.” It walks Claude through the steps of a field investigation from CDC’s *Field Epidemiology Manual*, adapted to a Massachusetts board of health working with DPH and MAVEN, and has Claude use the tools in this chapter in order.',
          },
          {
            p: 'In Claude Desktop, prompts offered by connected servers appear in the menu for adding content to a message (the exact place depends on your version). Choose “Investigate an outbreak” and fill in whatever you know; every item is optional:',
          },
          {
            bullets: [
              'the suspected disease or syndrome (for example “GI illness, agent unknown”);',
              'the setting (school, child care, long-term care, shelter, day program, restaurant or event, or community);',
              'the line list file in your data folder;',
              'the date and time of a known common exposure.',
            ],
          },
          {
            p: 'The workflow covers notifying DPH Epidemiology; confirming the diagnosis and the outbreak (section 6.9); defining and counting cases (6.7); describing time, place, and person (6.8); starting control measures (6.3, 6.4, 6.5, and 6.6); testing hypotheses (8.4); and communicating findings (5.3), with privacy rules for the whole investigation.',
          },
        ],
      },
    ],
  },
  {
    title: 'Immunizations',
    intro: 'Since 2025, federal CDC/ACIP guidance has differed in places from DPH and the professional societies (AAP, ACOG, and AAFP). The immunization tools therefore show each body’s position side by side, with its date. By default they list DPH first where it has a position, then AAP for children, ACOG for pregnancy, and AAFP for adults, then CDC/ACIP; you can change this order (section 4.1). The forecasting tool (section 7.3) is the exception: it applies CDC’s published ACIP logic only. These tools are in the vax group, which the school_nurse and cd_nurse profiles include; the medication lookup (section 7.10) is in the always-on core group. The immunization content was drafted from documents retrieved in November 2026 and is pending review.',
    tools: [
      {
        id: 'get_immunization_schedule',
        title: 'Look up immunization schedules',
        toolset: 'vax',
        internet: 'no',
        files: 'no',
        does: 'Shows the routine childhood (birth to 18 years) or adult schedule with the DPH, AAP/ACOG/AAFP, and CDC/ACIP positions side by side. For a named vaccine it adds CDC’s minimum ages and intervals and its contraindications and precautions.',
        when: [
          'Quick reference during a clinic or a phone call.',
          'Answering a parent’s or provider’s schedule question.',
          'Checking minimum intervals or contraindications before a clinic.',
        ],
        provide: [
          'A vaccine or disease (“MMR”, “hep B”, “Shingrix”), or nothing for every vaccine.',
          'Child or adult schedule, and whether you want minimum ages and intervals and contraindications.',
        ],
        examples: [
          'What is the routine childhood schedule for MMR, with minimum ages and intervals?',
          'What are the contraindications and precautions for varicella vaccine?',
          'Show me the adult schedule for all vaccines.',
        ],
        returns: [
          'The schedule with each body’s position, the schedule editions used and their dates, and general rules such as the 4-day grace period and spacing of live vaccines.',
        ],
        notes: ['It does not work out a particular person’s doses; use the forecasting tool in section 7.3.'],
      },
      {
        id: 'compare_vaccine_recommendations',
        title: 'Compare vaccine recommendations',
        toolset: 'vax',
        internet: 'no',
        files: 'no',
        does: 'Shows each body’s recommendation on a topic (DPH, AAP, ACOG, AAFP, and CDC/ACIP), with dates, sources, and confidence, and points out where they differ. It covers 17 topics, including the childhood and adult schedules, the hepatitis B birth dose, MMRV under age 4, COVID-19 (children, adults, and pregnancy), influenza, RSV, hepatitis A, rotavirus, MenACWY, Tdap in pregnancy, and thimerosal.',
        when: [
          'A parent, provider, or school asks why recommendations differ.',
          'Preparing a statement, FAQ, or briefing.',
          'Checking DPH’s position against current federal guidance.',
        ],
        provide: [
          'A topic or question, and optionally a population (infants, children, adolescents, adults, or pregnancy).',
          'Optionally, which body to list first for this answer only.',
        ],
        examples: [
          'How do recommendations for the hepatitis B birth dose differ between DPH, AAP, and CDC?',
          'What does each body recommend for COVID-19 vaccine in pregnancy?',
          'List the topics you can compare.',
        ],
        returns: [
          'Each body’s position with its date, source, and confidence (high when taken from the body’s own document; medium when described by another document), whether and how they differ, and which bodies have no position on file.',
        ],
        notes: ['Recommendations change; check the dates shown. Pending review.'],
      },
      {
        id: 'forecast_immunizations_due',
        title: 'Forecast immunizations due',
        toolset: 'vax',
        internet: 'no',
        files: 'only when checking a file of many people',
        does: 'Evaluates a vaccination history and forecasts which doses are due and when, using CDC’s Clinical Decision Support for Immunization (CDSi) logic and supporting data, which apply the ACIP schedule. It works for one person, or for a whole file of people with only totals returned to the chat.',
        when: [
          'A child’s record has gaps: what is due now, and what is the catch-up schedule?',
          'Checking whether a dose given early or too soon after another counts.',
          'Reviewing the records of a clinic’s or program’s population (file mode).',
        ],
        provide: [
          'One person: date of birth, sex (the HPV series differs by sex), the assessment date (today by default), and each dose’s vaccine and date. Vaccine names, trade names such as Pediarix, or CVX codes all work.',
          'Relevant conditions, such as pregnancy, asplenia, or work in health care, and whether the person was born in the United States.',
          'File mode: a CSV or Excel file with one row per dose and columns for a person ID, date of birth, vaccine, and date.',
        ],
        examples: [
          'What is due for a child born 2022-03-15 who received Pediarix on 2022-05-20, 2022-07-22, and 2022-09-30 and MMR on 2023-03-20?',
          'Was a DTaP dose given at 5 weeks of age valid?',
          'Run forecasts for everyone in clinic_doses.xlsx and tell me how many are overdue for MMR.',
        ],
        returns: [
          'For each vaccine group: a status (overdue, due now, can give now, due later, complete, immune, contraindicated, aged out, or not recommended now), the next dose number, and the earliest, recommended, and past-due dates.',
          'An evaluation of each dose (valid, not valid, or extra) with the reason, and a projected catch-up schedule.',
          'File mode: per-person results saved as CSV files in your output folder; the chat receives only the number of people in each status, with small-cell suppression.',
        ],
        notes: [
          'Leave the person’s name out; the date of birth and dose dates are all the tool needs, and the answer shows ages, not the date of birth.',
          'CDSi reflects CDC/ACIP only. DPH and the professional societies may differ (use section 7.2), and Massachusetts school requirements are a separate tool (section 7.4).',
          'The engine passes all 1,064 of CDC’s healthy-person test cases. 36 older test cases involving underlying conditions do not pass and are documented, so check risk-based forecasts carefully.',
          'Seasonal vaccines (influenza, COVID-19, RSV) show “not recommended now” after the season in CDC’s data ends, until CDC publishes new data.',
          'Always verify against the MIIS record before acting.',
          'In a file, doses dated before the date of birth or after the assessment date are not evaluated; the answer counts them, and people whose rows give different dates of birth, so you can correct the file.',
        ],
      },
      {
        id: 'check_ma_school_immunization_requirements',
        title: 'Check Massachusetts school immunization requirements',
        toolset: 'vax',
        internet: 'no',
        files: 'only when checking a roster',
        does: 'Checks one student or a whole roster against DPH’s 2026–2027 school immunization requirements (105 CMR 220.000). It first checks each dose against minimum ages and intervals, then reports each requirement as met, not met, not yet due, exempt, not applicable, or needs review, with the earliest date a missing dose would count.',
        when: [
          'School entry and kindergarten registration.',
          'Preparing for DPH’s school immunization survey.',
          'Finding students who need follow-up or exclusion, and reviewing transfer students or college students.',
        ],
        provide: [
          'One student: grade (PK, K, 1–12, college, or ungraded), date of birth, the date to evaluate (for example the first day of school), and dose dates by vaccine.',
          'Any laboratory evidence of immunity, a reliable history of chickenpox, and medical or religious exemptions.',
          'Optionally, whether to apply CDC’s 4-day grace period (on by default).',
          'A roster: a CSV or Excel file with Grade, Date of Birth, and one column per vaccine with dose dates separated by semicolons; optional Exemptions, Varicella History, and Lab Immunity columns.',
        ],
        examples: [
          'Does this kindergartner meet the MMR and varicella requirements for September 2, 2026? Born 2021-06-02; MMR on 2022-06-10 and 2025-06-15; varicella on the same dates.',
          'Check kindergarten_roster.xlsx against the school requirements as of September 2, 2026.',
          'What is the earliest date this student’s second dose would count?',
        ],
        returns: [
          'For each requirement: the status and reason, doses that do not count and why, and the earliest valid date for a missing dose.',
          'Roster mode: per-student results saved to school_immunization_check_<date>.xlsx in your output folder; the chat receives only counts by grade, requirement, and status, with small-cell suppression.',
        ],
        notes: [
          '“Needs review” marks cases the rules cannot decide, such as a Hib series whose product is unknown. MenB doses never count toward a requirement.',
          'DPH does not say whether the 4-day grace period applies to school requirements; the tool applies it by default and says so.',
          'DPH’s May 2026 flyer and its 2026–2027 requirements differ on the minimum age for MenACWY; the tool follows the requirements document.',
          'Pending review against DPH’s current requirements document.',
          'For a student 7 or older whose DTaP doses do not meet the requirement, the result is “needs review”, because Tdap or Td can complete a catch-up series; DTaP is never suggested at 7 or older.',
          'A dose given too soon after a dose that did not count is marked “needs review” rather than met.',
          'Roster columns may carry a dose number (“MMR 1”, “DTaP #2”). The answer lists the columns it read as vaccines and any it did not use; a requirement whose column was not used shows as not met, so check that list.',
        ],
      },
      {
        id: 'analyze_vaccination_coverage',
        title: 'Analyze school vaccination coverage',
        toolset: 'vax',
        internet: 'no',
        files: 'no',
        does: 'Ranks schools and towns using DPH’s school immunization survey (kindergarten and grade 7), flags schools below herd-immunity thresholds (95% for MMR), and compares Massachusetts with the United States using CDC estimates. It can also analyze school-level counts you provide.',
        when: [
          'Choosing schools for outreach or school-located clinics.',
          'Reports to the board of health or school committee.',
          'Comparing your town with the state, or tracking exemptions.',
        ],
        provide: [
          'A town, county, school, or statewide (your saved town by default).',
          'The grade: K, 7, or K3 (kindergarten 2023–2026 combined, which includes more small schools).',
          'The measure: MMR (default), DTaP, polio, hepatitis B, varicella, Tdap, meningococcal, complete series, exemptions, unimmunized, no record, noncompliance, or influenza (Massachusetts compared with the U.S. only).',
          'Optionally: a different threshold, how many schools to list, or your own file with columns for school, enrolled, and the count.',
        ],
        examples: [
          'Which kindergartens in Springfield are below 95% for MMR?',
          'Rank Worcester County seventh-grade schools by exemption rate.',
          'How does Massachusetts kindergarten MMR coverage compare with the U.S.?',
        ],
        returns: [
          'A ranked list (lowest coverage first; for exemptions and similar measures, highest first), the schools below the threshold, counts of schools that were not released or did not respond, and Massachusetts and U.S. estimates.',
        ],
        notes: [
          'DPH does not publish schools with fewer than 30 students or with unresolved discrepancies; they are counted but not ranked.',
          'Town figures are unweighted summaries of school rates, because DPH publishes rates without enrollment.',
          'In your own file, rates are shown only for schools with 30 or more students, and student-level files are refused.',
        ],
      },
      {
        id: 'vaccine_temperature_excursion_helper',
        title: 'Respond to a vaccine temperature excursion',
        toolset: 'vax',
        internet: 'no',
        files: 'only if you ask for a worksheet',
        does: 'When a vaccine refrigerator or freezer goes out of range, classifies the readings against the normal range and the Massachusetts Vaccine Program’s reporting thresholds, lists the steps from CDC’s *Vaccine Storage and Handling Toolkit* and DPH’s steps for state-supplied vaccine, gives contacts, and can create an excursion worksheet.',
        when: [
          'An alarm or data logger shows an out-of-range temperature.',
          'After a power outage or a unit door left open.',
          'Training staff on excursion response.',
        ],
        provide: [
          'The unit type (refrigerator, freezer, or ultra-cold), the lowest and highest readings, Celsius or Fahrenheit, and the total minutes out of range in 24 hours.',
          'Whether the unit holds state-supplied vaccine (assumed yes) and any open multidose vials.',
          'For the worksheet: when the excursion was discovered and each vaccine’s name, manufacturer, lot, expiration date, number of doses, and funding source, and whether you want Excel or Word.',
        ],
        examples: [
          'Our vaccine refrigerator read 9.5 °C at 7 a.m., and the data logger shows it was above 8 °C for about 95 minutes overnight. It holds state-supplied vaccine. What do we do?',
          'Make an excursion worksheet in Excel for that event.',
        ],
        returns: [
          'A classification (in range; out of range but below the reporting threshold; reportable for cumulative time; reportable immediately; or undetermined), the steps to take, contacts, and the worksheet if requested.',
        ],
        notes: [
          'The tool never decides whether vaccine can still be used. Only the manufacturer can; for state-supplied vaccine, the Vaccine Program then gives instructions.',
          'Readings entered in Fahrenheit are compared with the Fahrenheit range (36–46 °F for a refrigerator).',
        ],
      },
      {
        id: 'lookup_vaccine_product',
        title: 'Look up a vaccine product',
        toolset: 'vax',
        internet: 'optional',
        files: 'no',
        does: 'Finds vaccine products by name, trade name, CVX code, MVX manufacturer code, or NDC, and gives the CVX description and status, trade names and manufacturers, NDCs, licensed ages and storage where available, the current Vaccine Information Statement (VIS) edition, and the link to VIS translations.',
        when: [
          'Documenting a dose that needs a CVX code or NDC.',
          'Identifying a product from a record or a package.',
          'Checking the current VIS edition before a clinic, or finding a VIS in another language.',
        ],
        provide: [
          'A name, trade name, or code.',
          'Optionally: whether to include inactive codes, how many matches to show, and whether to fetch the current package insert from DailyMed (needs internet).',
        ],
        examples: [
          'What vaccine is CVX 03?',
          'What is the current VIS edition for MMR, and where can I find it in Spanish?',
          'Look up Pentacel and include the package insert.',
        ],
        returns: [
          'The matching products with their codes, manufacturers, NDCs, licensed ages and storage notes where available, VIS edition, and links.',
        ],
        notes: ['The CVX list is refreshed from CDC when online (and kept for a week); otherwise the bundled copy is used.'],
      },
      {
        id: 'plan_vaccination_clinic',
        title: 'Plan a vaccination clinic',
        toolset: 'vax',
        internet: 'no',
        files: 'yes, a dose reconciliation sheet (unless you turn it off)',
        does: 'Calculates staffing for each station, throughput, observation seating, vials and supplies, and the anaphylaxis kit with epinephrine dosing by age and weight, suggests a layout, and creates a dose reconciliation sheet.',
        when: [
          'Planning flu clinics, school-located clinics, or back-to-school catch-up clinics.',
          'Preparedness exercises for mass vaccination.',
        ],
        provide: [
          'Expected turnout and clinic hours, doses per person, the vaccines, and the population (children, adolescents, adults, or mixed).',
          'Optionally: doses per vial, your own rates (doses per vaccinator per hour, registrations or screenings per hour, observation minutes, waste percentage, extra observation seats), and a weight for an epinephrine dose.',
        ],
        examples: [
          'Plan a 4-hour school flu clinic for 400 students using 10-dose vials.',
          'We expect 250 adults for flu and COVID-19 vaccines over 3 hours. How many vaccinators and observation chairs do we need?',
          'What epinephrine dose would a child weighing 18 kg receive?',
        ],
        returns: [
          'Staff for each station and how each number was calculated, observation seats, vials and supplies, the anaphylaxis kit, protocol, and dosing tables, a layout, and the reconciliation sheet in your output folder.',
        ],
        notes: [
          'Every assumption is labeled: *source* (cited), *tool default* (a planning figure with no published source; replace it with your agency’s experience), or *your override*.',
          'The anaphylaxis tables were transcribed from DPH standing orders and Immunize.org guidance and are pending review. Follow your agency’s current standing orders.',
        ],
      },
      {
        id: 'vaccine_confidence_conversation_guide',
        title: 'Prepare for a vaccine confidence conversation',
        toolset: 'vax',
        internet: 'no',
        files: 'no',
        does: 'Prepares a motivational-interviewing response to a specific vaccine concern: an open question, a reflection, asking permission, one to three facts with sources, and a next step, written at a target reading level.',
        when: [
          'Preparing to talk with a hesitant parent or adult.',
          'Training staff, or writing FAQ content.',
        ],
        provide: [
          'The concern in the person’s own words (“worried about autism”, “too many shots”, “HPV at 11”).',
          'Optionally: a target reading grade (8 by default), the person’s language, and how many facts to include (1 to 6).',
        ],
        examples: [
          'A parent is worried that too many shots at once will overwhelm their baby’s immune system. Help me prepare.',
          'A parent thinks age 11 is too young for HPV vaccine. Give me a 6th-grade-level script.',
          'The parent prefers Spanish. What materials can I give them?',
        ],
        returns: [
          'The conversation sequence, facts cited to CDC, AAP, Immunize.org, or DPH with dates, and the reading grade of the script.',
          'A note where federal pages now differ from the professional societies.',
        ],
        notes: [
          'For other languages it links to authoritative translated materials; it never machine-translates.',
          'Topics covered: safety, ingredients, autism, too many too soon, the changed federal schedule, HPV, COVID-19, influenza, and pregnancy.',
        ],
      },
      {
        id: 'lookup_medication_information',
        title: 'Look up medication information',
        toolset: 'core',
        internet: 'yes',
        files: 'no',
        does: 'Looks up a medication’s names and ingredients (RxNorm), FDA label highlights, recalls, drug shortages, and DailyMed package inserts.',
        when: [
          'Checking a medication on a standing order or school medication form.',
          'Checking for a shortage of a medication used for treatment or prophylaxis.',
          'Answering a question about a recall.',
        ],
        provide: [
          'A brand or generic name.',
          'Optionally, which sections you want (names, label, recalls, shortages, package insert) and how many results per section.',
        ],
        examples: [
          'Is there a current shortage of amoxicillin?',
          'Show me the FDA label highlights and any recalls for epinephrine autoinjectors.',
          'What are the brand names for azithromycin?',
        ],
        returns: [
          'Names and ingredients, label highlights with the label’s date and version, recalls, shortages, and package insert links with their dates.',
        ],
        notes: [
          'Reference only; it gives no patient-specific dosing.',
          'It needs the internet. There is no offline copy, and any section it could not check is listed as not checked.',
          'It is in the core group, so it is always available.',
          'If the name you typed is not an exact match in RxNorm, the answer names the closest match and which name was searched. Confirm it is the drug you meant.',
        ],
      },
    ],
  },
  {
    title: 'Epidemiology and statistics',
    intro: 'These tools do the calculations behind reports, investigations, and evaluations. The math is done by tested program code and every answer explains the method and cites it. You can type aggregate numbers (counts and populations) into the chat, or point the tools to files in your data folder, in which case only aggregates are returned. Tools that make charts and maps save them in your output folder with alternative text and a data table. Every profile includes the epi group; the run and control chart tool (section 8.9) is in the ebp group.',
    tools: [
      {
        id: 'calculate_rates',
        title: 'Calculate rates with confidence intervals',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Calculates crude and group-specific rates (per 100,000 or any base) with exact Poisson confidence intervals, relative standard errors, and flags for numbers too small to be reliable.',
        when: [
          'Incidence or prevalence for an annual report or a board of health presentation.',
          'Rates by age group, sex, or school.',
          'Checking whether a rate is reliable enough to publish.',
        ],
        provide: [
          'For each group: a label, the count, and the population.',
          'Optionally: the rate base (per 100,000 by default), the confidence level (95% by default), and whether to include a total.',
        ],
        examples: [
          'What is the rate per 100,000 for 14 cases in a population of 38,500?',
          'Pertussis rates by age group: under 1, 3 cases in 410; 1–4, 5 cases in 1,700; 5–14, 12 cases in 4,300.',
        ],
        returns: [
          'Each rate with its confidence interval and relative standard error, and flags for zero counts, rates based on fewer than 20 events (unreliable), counts below your suppression threshold, and counts larger than the population.',
        ],
        notes: ['Rates based on fewer than 20 events are unstable; consider combining years or groups.'],
      },
      {
        id: 'age_adjust_rates',
        title: 'Age-adjust rates',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Directly age-adjusts rates to the 2000 U.S. standard population (using NCHS weights), with gamma-method confidence intervals, and compares groups with a reference group.',
        when: [
          'Comparing towns, counties, or years whose age structures differ, such as a retirement community and a college town.',
          'Community health assessments and improvement plans.',
          'Producing rates comparable to those DPH and CDC publish.',
        ],
        provide: [
          'For each group: counts and populations by age group (for example <1, 1–4, 5–14, … 85+).',
          'Optionally: a reference group for comparisons, a different standard population, the rate base, and the confidence level.',
        ],
        examples: [
          'Age-adjust our 2025 cancer death rates and compare them with Massachusetts. Here are deaths and population by age group: [paste the table]',
          'Compare age-adjusted diabetes hospitalization rates for 2020 and 2025, with 2020 as the reference.',
        ],
        returns: [
          'Crude and age-adjusted rates with confidence intervals, rate ratios and differences compared with the reference group with a test, and the source of the weights.',
        ],
        notes: ['Rates from fewer than 20 events and totals below the suppression threshold are flagged.'],
      },
      {
        id: 'compute_standardized_ratio',
        title: 'Compare observed with expected (SIR and SMR)',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Compares the number of cases or deaths observed in an area with the number expected if the area had the reference population’s rates (for example statewide rates), giving a standardized incidence ratio (SIR) or standardized mortality ratio (SMR) with an exact confidence interval and p-value.',
        when: [
          'Residents raise concern about a possible cancer cluster.',
          'Asking whether a town’s count is higher than expected given its age structure.',
          'If your age groups do not start at 0 and end with an open-ended oldest group (such as 85+), the answer warns that the rates describe only those ages and are not comparable with published age-adjusted rates.',
        ],
        provide: [
          'For each area: the observed count, and by age group (or other stratum) the local population and the reference rate (or reference count and population).',
          'Whether it is incidence (SIR, the default) or mortality (SMR), and the reference population’s name.',
        ],
        examples: [
          'Our town had 42 colorectal cancer cases in 2019–2023. Using these Massachusetts rates by age group and our population by age group, what is the SIR? [paste the table]',
        ],
        returns: ['The expected count, the ratio, its exact confidence interval and p-value, and an explanation.'],
        notes: [
          'Each area is compared with the reference, not with other areas.',
          'The tool points to CDC/CSTE guidance for investigating suspected clusters.',
        ],
      },
      {
        id: 'analyze_two_by_two',
        title: 'Analyze a 2×2 table',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'From counts of exposed and unexposed people who did or did not become ill, calculates attack rates, the risk ratio, odds ratio, risk difference, and attributable fraction, with chi-square and Fisher exact tests. With several strata it adds Mantel–Haenszel adjusted measures and tests for confounding and effect modification.',
        when: [
          'Testing a hypothesis in an outbreak, such as whether eating a certain food raised the risk of illness.',
          'A case-control or cross-sectional study.',
          'Checking for confounding by stratifying, for example by age group.',
        ],
        provide: [
          'The four counts (exposed ill, exposed well, unexposed ill, unexposed well), for each stratum if stratified.',
          'The design (cohort, case-control, or cross-sectional) and labels for the exposure and outcome.',
        ],
        examples: [
          'Of 40 people who ate the potato salad, 22 became ill; of 35 who did not, 4 became ill. Is the potato salad associated with illness?',
          'Run the same analysis stratified by adults and children: [paste the counts]',
        ],
        returns: [
          'The measures with confidence intervals and p-values and a plain-language interpretation.',
          'With strata: crude and adjusted measures, a test of whether the strata differ, and how much adjustment changed the result (a screen for confounding).',
        ],
        notes: [
          'Choose the design carefully: case-control studies give odds ratios, not risk ratios.',
          'For a whole line list, use the outbreak analysis tool (section 6.7), which runs this analysis for every exposure.',
          'For very large tables (counts in the millions), the exact tests are skipped and the chi-square tests are used; the answer says so.',
        ],
      },
      {
        id: 'run_statistical_test',
        title: 'Choose and run a statistical test',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Chooses the right test for your data (t-test, Mann–Whitney, ANOVA, Kruskal–Wallis, paired tests, chi-square, Fisher’s exact test, correlation, or linear or logistic regression), checks its assumptions, runs alternatives as a sensitivity check, and explains the result in plain language.',
        when: [
          'Program evaluation: comparing before and after.',
          'Comparing groups, such as screening results by school.',
          'Quality improvement and DNP projects.',
        ],
        provide: [
          'The data as groups of values, before-and-after pairs, a table of counts, or two measurements; or a file with the outcome and group or predictor columns.',
          'Optionally, the test you want; otherwise it chooses.',
        ],
        examples: [
          'Compare these blood pressure readings before and after our program: before [values]; after [values].',
          'Is the proportion of students up to date different among these three schools? [paste the counts]',
          'Using program_eval.xlsx, does attendance predict completion? Use logistic regression.',
        ],
        returns: [
          'The test chosen and why, the assumption checks, the result with a confidence interval and p-value, the alternative tests, and a plain-language interpretation.',
        ],
        notes: [
          'From files, only aggregates are returned (count, mean, standard deviation, median, and interquartile range; never minimum, maximum, or rows). Identifier columns and groups smaller than your suppression threshold are refused, and table cells are suppressed.',
          'If a logistic regression finds a predictor that predicts the outcome perfectly (separation), or the model does not converge, the answer warns you not to interpret the estimates.',
          'Data with no differences at all (every value the same) give p = 1. A correlation with a measurement that never changes is refused.',
        ],
      },
      {
        id: 'calculate_sample_size',
        title: 'Calculate sample size and power',
        toolset: 'epi',
        internet: 'no',
        files: 'no',
        does: 'Calculates the sample size or the statistical power for surveys (of a proportion or a mean), comparisons of two groups, case-control studies, and cluster samples, adjusting for expected response rate and design effect.',
        when: [
          'Planning a community or school survey.',
          'Planning a program evaluation, QI project, or DNP project.',
          'Checking whether the sample you can get is large enough to detect a difference.',
        ],
        provide: [
          'The type of study and the expected values, for example the expected proportion and margin of error, two proportions to compare, or an odds ratio and the exposure among controls.',
          'Optionally: confidence level, significance level, power (80% by default), group size ratio, one-sided test, population size, design effect or cluster size and intraclass correlation, and response rate.',
        ],
        examples: [
          'How many households do I need to survey to estimate flu vaccination coverage within ±5 percentage points? The town has 12,000 households and we expect a 60% response rate.',
          'How many people per group do I need to detect an increase in screening from 40% to 55% with 80% power?',
          'We can only get 150 people per group. What power do we have to detect 40% versus 55%?',
        ],
        returns: ['The sample size per group and in total, every adjustment applied, and the formula and its source.'],
        notes: ['The survey formula matches CDC Epi Info StatCalc (and Krejcie and Morgan’s table).'],
      },
      {
        id: 'analyze_trends',
        title: 'Analyze trends over time',
        toolset: 'epi',
        internet: 'no',
        files: 'yes, a trend chart (unless you turn it off)',
        does: 'Analyzes counts or rates over years, quarters, months, or weeks: the annual percent change, joinpoints (where the trend changed direction or speed), interrupted time series (before and after a known change such as a new program or policy), and seasonality, with a trend chart.',
        when: [
          'Has a rate gone up or down over the past decade?',
          'Did a program or policy change the level or the trend?',
          'Annual reports and community health assessments.',
        ],
        provide: [
          'Counts for each period in order, with populations if you want rates, and the time unit.',
          'Optionally: the first period after a known change, the maximum number of joinpoints, the rate base, and whether to draw a chart.',
        ],
        examples: [
          'Here are chlamydia cases and population for 2014–2025. What is the annual percent change, and did the trend change? [paste the data]',
          'Our school-based asthma program started in September 2022. Using these monthly asthma emergency visits, did the level or the slope change after that? [paste the data]',
        ],
        returns: [
          'The annual percent change with its confidence interval; the joinpoint model with each segment’s change and the average; changes in level and slope at known interruptions; a seasonality test; a trend test; warnings about autocorrelation; and a chart in output/charts.',
        ],
        notes: [
          'Joinpoint locations are chosen by a different statistical criterion than NCI’s Joinpoint program uses, so results can differ slightly from it.',
          'Short series allow few joinpoints.',
          'Monthly and weekly data are adjusted for seasons only with at least two full years; with less, the answer says that a seasonal pattern may look like a trend. A series of all zeros is refused.',
        ],
      },
      {
        id: 'create_chart',
        title: 'Create a chart',
        toolset: 'epi',
        internet: 'no',
        files: 'yes, in output/charts',
        does: 'Creates accessible bar, stacked bar, line, dot, and histogram charts (PNG and SVG) in colors chosen to be distinguishable with color-vision deficiency, with alternative text and a data table.',
        when: ['Reports, presentations, board of health updates, and fact sheets.'],
        provide: [
          'The chart type, a title, axis labels, the categories, and up to 8 data series (with confidence limits if you have them).',
          'Optionally: reference lines, markers, a source note, a log scale, hiding small counts, and PNG, SVG, or both.',
        ],
        examples: [
          'Make a bar chart of pertussis cases by age group with 95% confidence intervals.',
          'Make a line chart of monthly influenza-like illness visits for 2024 and 2025 with a reference line at the baseline.',
          'Put that chart in a Word report on our letterhead.',
        ],
        returns: ['The files, the alternative text, the data table, and the text needed to place the chart in a document (section 5.3).'],
        notes: ['Bar charts always start at zero. Hidden small counts are shown hatched.'],
      },
      {
        id: 'create_spc_chart',
        title: 'Create a run chart or control chart',
        toolset: 'ebp',
        internet: 'no',
        files: 'yes, a chart in your output folder',
        does: 'Creates run charts and statistical process control charts (XmR, p, u, and c charts) that separate real change (“special cause” variation) from ordinary variation, with baselines and phases for before and after a change.',
        when: [
          'Quality improvement projects and PDSA cycles, such as the percentage of reports investigated within 24 hours or clinic wait times.',
          'Monitoring any process over time.',
          'Bar charts cannot use a log scale. When a value is hidden by small-cell suppression, its confidence limits are hidden too.',
        ],
        provide: [
          'The chart type: run chart (default), XmR for measurements, p for proportions, u for rates, or c for counts.',
          'The values and period labels (with denominators for p and u charts), or a local file with a date column, the period (week, month, or quarter), and what to measure.',
          'Optionally: how many points form the baseline, where new phases start, a title, and showing proportions as percentages.',
        ],
        examples: [
          'Make a run chart of the percentage of new cases contacted within 24 hours, by month: [paste the data]',
          'Make a p-chart by month from immunization_visits.xlsx of the proportion up to date, with a new phase starting March 2026.',
        ],
        returns: [
          'The chart, its center line and limits, any signals (for run charts, the Anhøj rules; for control charts, the *Health Care Data Guide* rules, such as a point outside the limits or 8 in a row on one side), and an interpretation.',
        ],
        notes: [
          'This tool is in the ebp group, which only the phn_leader profile includes; turn the group on if you need it.',
          'For local files, values based on fewer records than your suppression threshold are hidden in the chat (the saved chart keeps them, on your computer).',
          'When values from a local file are hidden, the center line and control limits are also left out of the chat (they would reveal the hidden values); the saved chart has them.',
          'Repeated values and points on the center line do not count toward a trend or a shift.',
        ],
      },
      {
        id: 'create_choropleth_map',
        title: 'Create a map',
        toolset: 'epi',
        internet: 'no',
        files: 'yes, in output/maps',
        does: 'Creates accessible shaded (choropleth) maps of Massachusetts towns, census tracts, ZIP code areas, or school districts, with natural-breaks or quantile classes and hatching for areas with suppressed or missing data.',
        when: [
          'Showing rates or coverage across the towns of a region or the tracts of a city.',
          'Community health assessments and targeting outreach.',
        ],
        provide: [
          'The geography and a title.',
          'The values for each area (typed, or an aggregated file with one row per area), and whether the values are counts (small counts are then hidden).',
          'Optionally: the classification method, the number of classes (3 to 7), and whether to show the whole state or zoom to the mapped areas.',
        ],
        examples: [
          'Map kindergarten MMR coverage by town in Hampden County.',
          'Map the overall SVI percentile for each census tract in Lawrence.',
          'Make a map from rates_by_zip.xlsx.',
        ],
        returns: ['SVG and PNG maps in output/maps, alternative text, and a table of the classes.'],
        notes: [
          'Only aggregated data can be mapped; files with identifiers, repeated areas, or more than 5,000 rows are refused as record-level data.',
          'Boundaries: Census 2024 towns and tracts and 2020 ZIP code areas. School districts are built from member towns using a 2019 layer, so newer regionalizations are missing.',
        ],
      },
    ],
  },
  {
    title: 'Community health and equity',
    intro: 'These tools bring public data on population, social conditions, and chronic disease to your community health assessments, grant applications, and equity work. Every figure is cited, survey estimates come with their margins of error, and the tools work offline from bundled snapshots when live data cannot be reached. The first four tools are in the equity group (included in the cd_nurse, phn_leader, and preparedness profiles); the PLACES tool is in the chronic group (phn_leader only).',
    tools: [
      {
        id: 'build_community_health_profile',
        title: 'Build a community health profile',
        toolset: 'equity',
        internet: 'optional',
        files: 'no',
        does: 'Builds a one-request profile of a Massachusetts city or town, county, or census tract: population and age, race and ethnicity, nativity and language, income and poverty, health insurance and disability, housing, vehicles, internet access, and education, each with its American Community Survey table, period, and margin of error, side by side with Massachusetts and the United States. It adds CDC PLACES chronic disease estimates and the Social Vulnerability Index.',
        when: [
          'Starting a community health assessment.',
          'Grant applications and reports.',
          'Orienting new staff, or planning outreach.',
        ],
        provide: [
          'A town (your saved town by default), a county such as “Hampden County”, or an 11-digit census tract number.',
          'Optionally, whether to include the chronic disease estimates and SVI (both included by default).',
        ],
        examples: [
          'Build a community health profile for our town.',
          'Profile Hampden County and compare it with Massachusetts.',
          'Build a profile of our town and save it as a Word report on our letterhead.',
        ],
        returns: [
          'The profile, with each figure’s source table, period, and margin of error, and Massachusetts and U.S. values. Figures with a coefficient of variation above 30% (unreliable) are marked † and listed in a caveat.',
        ],
        notes: [
          'Births and deaths are not included, because no machine-readable town-level source was available; the answer says so.',
          'Live Census data sometimes need an API key on agency networks; without one the tool uses the bundled snapshot and says why.',
          'A median household income above the Census Bureau’s top value is shown as “$250,000 or more”.',
        ],
      },
      {
        id: 'get_social_vulnerability_index',
        title: 'Look up the Social Vulnerability Index',
        toolset: 'equity',
        internet: 'no',
        files: 'no',
        does: 'Gives the CDC/ATSDR Social Vulnerability Index (SVI) 2022 percentile ranks for the census tracts in a town or county, overall and for its four themes (socioeconomic status, household characteristics, racial and ethnic minority status, and housing type and transportation), ranked within Massachusetts or nationally.',
        when: [
          'Emergency preparedness planning.',
          'Choosing locations for clinics or outreach.',
          'Grant applications that ask about vulnerable populations.',
        ],
        provide: [
          'A town (your saved town by default), county, or tract.',
          'Optionally: ranking within Massachusetts (default) or nationally, the theme to sort by, and how many tracts to list (10 by default).',
        ],
        examples: [
          'Which census tracts in Worcester are most socially vulnerable?',
          'Rank Barnstable County tracts by the housing type and transportation theme.',
        ],
        returns: [
          'Each tract’s overall and theme percentile ranks (0 is least vulnerable, 1 most), the number of flagged variables, counts by quartile, and the median rank. For a single tract, its 16 underlying measures.',
        ],
        notes: [
          'Ranks are relative and cannot be compared across SVI releases. Massachusetts and national rankings differ.',
        ],
      },
      {
        id: 'language_access_profile',
        title: 'Plan language access',
        toolset: 'equity',
        internet: 'optional',
        files: 'no',
        does: 'Shows how many people in an area speak each language and how many speak English less than “very well” (limited English proficiency, LEP), with margins of error, and checks each language group against the HHS safe harbor for written translation (5% of the eligible population or 1,000 persons, whichever is less).',
        when: [
          'Deciding which languages to translate vital documents into.',
          'Planning interpreters for clinics and events.',
          'Writing or updating a language access plan.',
        ],
        provide: [
          'A town (your saved town by default), county, or tract.',
          'Optionally, the size of the population your program serves; the tool then scales each language group assuming the area’s language mix.',
        ],
        examples: [
          'Which languages meet the safe harbor for translation in Lowell?',
          'Our program serves 2,400 people in Chelsea. Which languages should our vital documents be translated into?',
        ],
        returns: [
          'For each of 12 language groups: speakers, people with limited English proficiency with the margin of error, and whether the group meets the safe harbor (meets even at the low end of the margin, meets, possibly, or no), plus the guidance’s rule for groups of fewer than 50 persons.',
          'Limited-English households, detailed languages for the surrounding area, and the 15 most common LEP languages in Massachusetts.',
        ],
        notes: [
          'The safe-harbor entry is marked low confidence because the HHS guidance could not be retrieved to verify it; the tool says so in every answer.',
          'Decision support, not legal advice. Pending review.',
        ],
      },
      {
        id: 'measure_health_disparities',
        title: 'Measure health disparities',
        toolset: 'equity',
        internet: 'no',
        files: 'no',
        does: 'Compares rates across groups (such as race and ethnicity, towns, or schools): rate ratios and rate differences with exact confidence intervals, the index of disparity, the between-group variance, excess events, and the population attributable fraction.',
        when: [
          'Disparity measures for a community health assessment.',
          'Equity reporting and program evaluation.',
        ],
        provide: [
          'For 2 to 50 groups: a label, the count, and the population; or an aggregated file with one row per group.',
          'The reference group (the most favorable rate by default, the largest group, or one you name), and whether a higher rate is better (as for screening or vaccination).',
        ],
        examples: [
          'Compare these asthma emergency visit rates by race and ethnicity, using the group with the lowest rate as the reference: [paste the table]',
          'Using vax_by_school.xlsx, measure disparities in flu vaccination; higher is better.',
        ],
        returns: ['Each measure with its method and citation, and a plain-language summary.'],
        notes: [
          'It uses crude rates. When groups differ in age, age-adjust first (section 8.2).',
          'Comparisons of American Community Survey estimates are not done here; compare them using the margins of error in the community health profile.',
          'From files, groups with small counts are left out of every measure so their counts cannot be recovered, and record-level files are refused.',
        ],
      },
      {
        id: 'get_places_estimates',
        title: 'Get CDC PLACES chronic disease estimates',
        toolset: 'chronic',
        internet: 'optional',
        files: 'no',
        does: 'Gives CDC PLACES model-based estimates for 40 measures (health outcomes, prevention, health risk behaviors, disability, health status, and health-related social needs) for a county, town, census tract, or ZIP code area, with 95% confidence intervals and comparisons with Massachusetts and the United States.',
        when: [
          'You need local estimates where there is no local survey, for example of diabetes, obesity, depression, or lack of health insurance.',
          'Community health assessments, grant applications, and choosing priorities.',
        ],
        provide: [
          'An area: a town, “X County”, a tract number, or a 5-digit ZIP code.',
          'Optionally: specific measures or a category, and age-adjusted estimates (counties and places only).',
        ],
        examples: [
          'What are the PLACES estimates for diabetes and obesity in Fall River?',
          'Show the health-related social needs measures for ZIP code 01840.',
          'Give me age-adjusted chronic disease estimates for Berkshire County.',
        ],
        returns: [
          'Each measure’s estimate, confidence interval, survey data year, and release year, with Massachusetts and U.S. values and whether the area is higher, lower, or similar.',
        ],
        notes: [
          'These are model-based small-area estimates, not results of a local survey.',
          'The Massachusetts and U.S. values are calculated by the server, not official CDC state figures. Some town values are derived from tract estimates and have no interval.',
          'This tool is in the chronic group, which only the phn_leader profile includes.',
        ],
      },
    ],
  },
  {
    title: 'Evidence-based practice',
    intro: 'These two tools find current evidence and federal guidance. Both are in the ebp group, which the phn_leader profile includes; turn the group on if your profile does not. Both need the internet.',
    tools: [
      {
        id: 'search_pubmed',
        title: 'Search PubMed',
        toolset: 'ebp',
        internet: 'yes',
        files: 'no',
        does: 'Builds and runs a PubMed search from the elements of a PICO question (population, intervention, comparison, and outcome), using MeSH headings and free-text terms with study-type filters, and returns abstracts, DOIs, and the exact search so you can rerun it.',
        when: [
          'Looking for evidence for a program, policy, or practice change.',
          'A literature review for a QI or DNP project, or journal club.',
        ],
        provide: [
          'The PICO elements, with synonyms or MeSH headings if you know them, or a ready-made PubMed query.',
          'Optionally: study types (systematic review, meta-analysis, randomized controlled trial, guideline, humans), years, languages, the number of results (10 by default), and relevance or newest first.',
        ],
        examples: [
          'Find systematic reviews since 2018 on school-located vaccination programs and adolescent vaccination coverage.',
          'Search PubMed: population older adults, intervention nurse home visits, outcome hospital readmission; randomized trials only.',
        ],
        returns: [
          'The exact query with a link to rerun it, the total number of matches, and for each article the PMID, title, journal, year, first three authors, an abstract excerpt, DOI, publication type, and link.',
        ],
        notes: [
          'Read the full articles before relying on them.',
          'MeSH headings that do not exist are dropped and reported. Recent searches are kept for a day, so they also work offline.',
          'If PubMed reports an error (for example a timeout), you get the error instead of an empty result; try again later.',
        ],
      },
      {
        id: 'search_cdc_guidance',
        title: 'Search CDC MMWR reports and health alerts',
        toolset: 'ebp',
        internet: 'yes',
        files: 'no',
        does: 'Finds the newest MMWR reports and Health Alert Network (HAN) notices on a topic, with dates, links, and CDC’s summaries, and gives a link to search CDC Stacks.',
        when: [
          'What is CDC’s latest on a topic?',
          'A HAN notice was issued and you want to read it.',
          'Background for a briefing or a staff update.',
        ],
        provide: [
          'A topic, and optionally which series (MMWR, HAN, or Stacks), a start date, and how many results per series.',
        ],
        examples: [
          'What are the latest MMWR reports and HAN notices on measles?',
          'Have there been any HAN notices about Candida auris since January 2025?',
        ],
        returns: [
          'Results newest first with title, date, series, link, and CDC’s summary, and how each series was searched and whether it could be reached.',
        ],
        notes: [
          'CDC Stacks does not allow automated searches, so the tool gives a search link instead of results.',
          'Follow DPH where Massachusetts requirements differ from CDC guidance.',
        ],
      },
    ],
  },
  // Phase 7 chapter goes below this line.
  {
    title: 'School health',
    intro: 'These eight tools support school nurses and child care health consultants: what screenings Massachusetts requires and who is due, medication administration rules, health plans, concussion management, illness absenteeism, the year-end report, and child care licensing checks. They are in the school group, which the school_nurse profile includes. All of their reference content was drafted from the regulations and DPH guidance in November 2026 and is waiting for your review. Tools that write letters or plans for families use English templates; your agency can add its own approved translations in a templates folder (the notes for each tool say how).',
    tools: [
      {
        id: 'get_ma_school_screening_requirements',
        title: 'Look up school screening requirements',
        toolset: 'school',
        internet: 'no',
        files: 'no',
        does: 'Lists the health screenings Massachusetts requires in schools (vision, hearing, height and weight for BMI, postural, SBIRT substance use screening, the kindergarten vision certificate, and physical examinations) by grade, with DPH protocols, referral rules, parent notification, exemptions, reporting to DPH, and confidentiality rules, each cited to 105 CMR 200.000, M.G.L. c. 71, or a DPH protocol.',
        when: [
          'Planning the screening calendar for the year.',
          'A parent or principal asks which screenings a grade gets.',
          'Checking the vision critical line or which charts DPH allows.',
          'Answering questions about BMI confidentiality or SBIRT opt-out.',
        ],
        provide: [
          'Optionally a grade (PK, K, 1–12, or ungraded with an age) and a screening name.',
          'Whether the student is new to the district (school-entry rules).',
        ],
        examples: [
          'Which screenings are required in grade 7?',
          'What are the vision screening rules for kindergarten, and what is the critical line?',
          'Show me the screening schedule for every grade.',
          'What does the law say about SBIRT confidentiality?',
        ],
        returns: [
          'For each screening: whether it is required in the grade and why, the schedule by grade band, who may screen, the protocol, referral and follow-up, parent notification, exemptions, reporting to DPH, and confidentiality, each with its citation.',
          'Without a grade: a table of screenings by grade.',
        ],
        notes: [
          'DPH’s current hearing protocol could not be retrieved, so hearing details come from DPH’s 2007 manual and are marked medium or low confidence.',
          'These are state minimums; your school committee or board of health may require more.',
          'Pending review.',
        ],
      },
      {
        id: 'track_school_screenings',
        title: 'Track screenings and send referral letters',
        toolset: 'school',
        internet: 'no',
        files: 'yes (your roster with results)',
        does: 'Reads your roster with screening results, works out which students are due for each required screening this school year, and computes completion, referral, and referral follow-up rates by grade, plus BMI categories for DPH’s BMI report. It writes an action list to your output folder and, if you ask, mail-merges referral letters in each family’s language.',
        when: [
          'Mid-year: who still needs vision or hearing screening?',
          'Following up on referrals that have not come back.',
          'Preparing BMI numbers for DPH.',
          'Sending referral letters to families in their language.',
        ],
        provide: [
          'A CSV or Excel file with a Grade column and, for each screening, a date, a result (pass, refer, absent, refused, exempt), and a follow-up column, for example Vision Date, Vision Result, Vision Referral Completed.',
          'For BMI: Height and Weight (or BMI Percentile or Category), with Date of Birth and Sex so the percentile can be computed.',
          'For letters: Student Name (or First and Last Name), Home Language, Parent Name, and your name and phone number.',
        ],
        examples: [
          'Check screenings.xlsx as of today and tell me completion rates by grade.',
          'Write referral letters for students whose vision referrals are still open.',
          'How many grade 4 students still need BMI measurements?',
        ],
        returns: [
          'Counts and rates by grade and screening (required, screened, referred, follow-up done, due, exempt), with small counts suppressed.',
          'An action list and per-student results in school_screenings_<date>.xlsx, and letters grouped by language in a letters folder, all in your output folder.',
          'How many letters used your agency’s translation and how many fell back to English.',
        ],
        notes: [
          'Student names never reach the AI; they are only in the files written to your output folder.',
          'Do not keep SBIRT results with names: the law forbids it, and the tool ignores such columns.',
          'Add your agency’s translations as templates/<language>/vision_referral.md (also hearing_referral and postural_referral); the English template is written next to the letters.',
          'Results such as “negative” or “no concerns” count as passed. Heights written as feet and inches (4’10”) are not read; give inches or centimeters.',
          'The language column must be named Home Language, Primary Language, Family Language, Preferred Language, or Language. Languages the server does not recognize are counted as “Other”.',
          'More small counts are hidden than before, so that a hidden number cannot be worked out by subtracting other numbers in the same row.',
        ],
      },
      {
        id: 'school_medication_administration_guide',
        title: 'Medication administration in schools',
        toolset: 'school',
        internet: 'no',
        files: 'no',
        does: 'Answers questions about giving medications in Massachusetts schools from 105 CMR 210.000 as amended in December 2025 and M.G.L. c. 71, section 54B, with each point cited by section: orders and parent consent, delegation to unlicensed staff, field trips, self-carry, storage, documentation, medication errors, epinephrine, naloxone, glucagon and other rescue medications, and DPH registration.',
        when: [
          'A principal asks whether a teacher can give medication on a field trip.',
          'Setting up delegation or training for unlicensed staff.',
          'A medication error happened and you need the reporting steps.',
          'Reviewing your district’s medication policy.',
        ],
        provide: [
          'A question in plain language, or a topic name. Ask with nothing to see the list of topics.',
        ],
        examples: [
          'Can a trained secretary give an EpiPen on a field trip?',
          'How long can we keep a student’s medication at school, and at what refrigerator temperature?',
          'Who can carry naloxone at school?',
          'What must a medication order contain?',
        ],
        returns: [
          'The matching topics with a short summary and cited points from the regulation and statute.',
        ],
        notes: [
          'Your school committee’s policy and nursing regulations also apply. Child care programs follow EEC’s rules instead (see the child care health consultation checklist in this chapter).',
          'Pending review.',
        ],
      },
      {
        id: 'build_individualized_health_plan',
        title: 'Build an individualized health plan',
        toolset: 'school',
        internet: 'no',
        files: 'only when filling plans from your student file',
        does: 'Drafts an individualized health plan and an emergency action plan for a student with diabetes, asthma, seizures, or a food allergy, from cited CDC and DPH guidance: how to recognize an emergency, what to do, when to call 911, the nursing assessment, goals, interventions, what the medication plan must contain under 105 CMR 210.000, and suggested Section 504 accommodations. If you point it at a file with your students’ details, it fills one plan per student in your output folder.',
        when: [
          'A new student with a chronic condition is enrolling.',
          'Annual plan renewal before school starts.',
          'Preparing for a 504 meeting.',
        ],
        provide: [
          'The condition.',
          'To fill plans: a CSV or Excel file with one row per student (Student Name, Date of Birth, Grade, Parent Name and Phone, Provider, Emergency Medication, Home Language, and condition details such as Allergens or Target Range).',
        ],
        examples: [
          'Give me an IHP template for a student with type 1 diabetes.',
          'Fill food allergy plans for the students in ihp_students.xlsx.',
          'What 504 accommodations are suggested for a student with asthma?',
        ],
        returns: [
          'The plan sections with citations and the list of fields to fill.',
          'When filling: one document per student (plan plus emergency action plan) in your output folder, with counts of blank fields; nothing about the students comes back to the chat.',
        ],
        notes: [
          'Never type a student’s details into the chat; put them in a file and point the tool at it.',
          'The provider’s orders govern treatment, you set the nursing goals, and the 504 team decides accommodations.',
          'Add your agency’s translated emergency action plans as templates/<language>/<condition>_emergency_plan.md.',
          'Pending review.',
          'A student whose Condition cell names more than one condition (“asthma, peanut allergy”) gets a plan for each condition you ask for.',
        ],
      },
      {
        id: 'concussion_return_to_learn_tracker',
        title: 'Concussion: return to learn and return to play',
        toolset: 'school',
        internet: 'no',
        files: 'only when checking your head injury log',
        does: 'Applies 105 CMR 201.000 and DPH and CDC guidance to a student’s head injury: removal from play, notification deadlines, forms, the graduated reentry plan, return-to-learn stages, accommodations for the student’s symptoms, a symptom-check schedule, and the gradual return to play. It flags problems such as clearance before the student is back in class full time. With your head injury log it checks every record, writes reentry plans and caregiver letters, and gives the counts DPH asks for each year.',
        when: [
          'A student was removed from a game with a suspected concussion.',
          'Writing a graduated reentry plan.',
          'Checking whether a clearance form can be accepted.',
          'Preparing the year-end head injury report.',
        ],
        provide: [
          'Dates only (no name): injury, diagnosis, return to school, symptom free, full-time classes, clearance, and the return-to-play stage completed; current symptoms if you want accommodations.',
          'For the log: a CSV or Excel file with one row per injury (Injury Date, Extracurricular, Written Notice, AD Nurse Notified, Head Injury Form, Diagnosed Concussion, Reentry Plan, Full Academics, Clearance Date, Cleared By, Full Return).',
        ],
        examples: [
          'A soccer player was hit on Friday, October 2, and diagnosed the next day. What is due and when?',
          'She has headaches and light sensitivity. What accommodations should teachers use?',
          'Check concussion_log.xlsx and write reentry plans for the diagnosed students.',
        ],
        returns: [
          'A dated timeline with each requirement’s status, the stages, accommodations, forms with links and translations, danger signs, and flags.',
          'Log mode: counts of records missing each requirement and the 105 CMR 201.017 statistics, with details, plans, and letters in your output folder.',
        ],
        notes: [
          'DPH’s 2018 guidance and CDC’s 2025 advice differ on how many days a student should stay home; the tool shows both.',
          'Business-day deadlines skip weekends but not holidays.',
          'Return to play is a medical decision; the tool tracks steps and never clears a student.',
          'Pending review.',
          'A notice column marked “yes” without a date counts as done (date not recorded), not as a gap. Symptom checks that would fall on a weekend move to the next weekday.',
        ],
      },
      {
        id: 'monitor_school_absenteeism',
        title: 'Monitor illness absenteeism',
        toolset: 'school',
        internet: 'no',
        files: 'yes (your absence records)',
        does: 'Looks for unusual increases in influenza-like or stomach-illness absences by school, and optionally by grade, classroom, or team, using CDC’s EARS methods, and tells you when and how to notify your local board of health.',
        when: [
          'Daily or weekly surveillance during flu and norovirus season.',
          'Several students went home vomiting today: is this unusual?',
          'Checking whether one team or classroom has a cluster.',
        ],
        provide: [
          'A file of absences: one row per absent student per day (Date, School, Reason), or daily counts by school (Date, School, Count, Reason or columns such as ILI and GI, and Enrollment).',
          'Optionally a grade, classroom, or team column to check within groups.',
        ],
        examples: [
          'Check absences.xlsx for unusual illness absences in the last week.',
          'Is there a stomach bug cluster at the middle school? Look by grade.',
        ],
        returns: [
          'Signals by school, syndrome, and day, recent counts (small counts shown as “<5”), how reasons were sorted, and the reporting rules: 105 CMR 300.134, DPH’s online cluster form for stomach illness, and child care parent notification.',
          'Exact daily counts in an Excel file in your output folder.',
        ],
        notes: [
          'A signal is a prompt to look closer and call the board of health, not a decision that there is an outbreak. DPH sets no numeric threshold.',
          'Names and IDs in your file are never read.',
          'Needs at least 8 school days of data.',
        ],
      },
      {
        id: 'build_school_health_annual_report',
        title: 'Build the school health annual report',
        toolset: 'school',
        internet: 'no',
        files: 'yes (your visit log, medication log, or screening roster)',
        does: 'Compiles the school year’s statistics from your health office visit log, medication log, and screening roster: visits by type, the returned-to-class and dismissal rates, 911 calls, head injuries, medication doses, screenings and referrals, and BMI counts by grade, sex, and category. It writes a workbook laid out by DPH’s published reporting categories and a summary for the school committee, and lists the DPH reports you may owe.',
        when: [
          'End of the school year, before DPH’s reports are due.',
          'Preparing a presentation for the school committee.',
        ],
        provide: [
          'A visit log (Date, Reason, Disposition; optional Grade, Type, 911 Type, Head Injury, Student ID), and optionally a medication log and the screening roster.',
          'The school year and enrollment.',
        ],
        examples: [
          'Build the 2025–2026 annual report from visits.xlsx and screenings.xlsx; enrollment is 420.',
          'What was our returned-to-class rate this year?',
        ],
        returns: [
          'Monthly and yearly counts and rates with small counts suppressed, BMI data-quality checks, and the list of DPH reports with due dates.',
          'A workbook and a school committee summary in your output folder.',
        ],
        notes: [
          'DPH does not publish its current year-end Status Report form, so match the workbook to DPH’s form before submitting.',
          'Reasons and dispositions are sorted by keywords; check the workbook.',
          'Pending review.',
          'If you do not name the school year and your logs have no visits in the current one (for example in July or August), the report covers the school year that just ended and says so.',
        ],
      },
      {
        id: 'child_care_health_consultation_checklist',
        title: 'Child care health consultation checklist',
        toolset: 'school',
        internet: 'no',
        files: 'only if you ask for a worksheet',
        does: 'Gives a checklist of the health-related EEC licensing requirements (606 CMR 7.00) for family, small group, or large group child care: the health care policy, the consultant’s approvals, medication administration and consent, individual health care plans, immunization and health records, illness exclusion and notification, infection control, first aid and emergencies, safe sleep, food allergies, and educator health, each cited with the evidence to look for.',
        when: [
          'Preparing for or documenting a health consultant visit.',
          'Reviewing a program’s health care policy for licensing renewal.',
        ],
        provide: [
          'The program type and, optionally, the areas to cover.',
          'Your findings (met, partly met, not met) if you want a gap list.',
        ],
        examples: [
          'Give me the medication administration checklist for a large group program.',
          'Record these findings and list the gaps: medication training not met, safe sleep met.',
          'Write a visit worksheet for a family child care program.',
        ],
        returns: [
          'The checklist items with citations and evidence to look for, counts by area, a gap list, and optionally a worksheet in your output folder.',
        ],
        notes: [
          'Based on 606 CMR 7.00 as published in December 2021; check EEC’s site for amendments.',
          'Do not record children’s names in findings.',
          'Pending review.',
        ],
      },
    ],
  },
  // End of Phase 7 chapter.
  // Phase 8 chapter goes below this line.
  {
    title: 'Maternal, child, and family health',
    intro: 'These tools support home visiting, early childhood, and lead poisoning prevention work: growth charts, lead screening and case management, mandated reporting, developmental milestones, newborn and postpartum visits, benefit eligibility screening, lead risk maps, and maternal and infant health data. They are in the mch group (benefit eligibility is in the equity group). Clinical and legal content is a draft until you review it.',
    tools: [
      {
        id: 'calculate_growth_percentiles',
        title: 'Calculate growth percentiles',
        toolset: 'mch',
        internet: 'no',
        files: 'only when scoring a file of many children',
        does: 'Calculates percentiles and z-scores for weight, length or height, BMI, and head circumference using the WHO standards from birth to 24 months and the CDC charts from 2 to 20 years, including CDC’s 2022 extended BMI-for-age method for children above the 95th percentile, the BMI category, and corrected age for babies born early. It compares visits and flags values to re-measure.',
        when: [
          'A home visit or clinic weight check for an infant or child.',
          'Following a baby’s weight gain over several visits.',
          'Scoring a file of measurements from a screening event or a WIC or school list.',
        ],
        provide: [
          'The child’s sex and, for each visit, the age (days, months, or years, or the measurement date with the birth date) and the measurements in kilograms or pounds and centimeters or inches.',
          'Optionally: gestational age at birth (for corrected age), or the name of a CSV or Excel file in your data folder for many children.',
        ],
        examples: [
          'A 9-month-old boy weighs 9.7 kg and is 72 cm long. What are his percentiles?',
          'Plot this baby girl’s weights: 2 weeks 3.4 kg, 1 month 4.0 kg, 2 months 4.9 kg. She was born at 34 weeks.',
          'A 10-year-old girl is 55 inches tall and weighs 110 pounds. What is her BMI percentile and category?',
          'Score the measurements in growth_screening.xlsx and give me the weight-status counts.',
        ],
        returns: [
          'For each visit: each measurement’s percentile, z-score, the chart used (WHO or CDC), the median, flags (for example below the 2.3rd percentile on the WHO chart, or the BMI category), and the change since the last visit.',
          'For a file: a spreadsheet in your output folder with each row’s results, and only counts by age group and weight status in the chat.',
        ],
        notes: [
          'Percentiles are screening information, not a diagnosis. Look at the pattern over time and refer to the child’s health care provider for concerns.',
          'The WHO charts flag growth at ±2 SD (the 2.3rd and 97.7th percentiles); the CDC charts traditionally use the 5th and 95th.',
          'Corrected age is used until 24 months for babies born before 37 weeks. This rule is a draft: confirm your agency’s practice.',
          'Birth and measurement dates are used only to calculate age; they are not shown in the answer.',
        ],
      },
      {
        id: 'childhood_lead_screening_guide',
        title: 'Childhood lead screening guide',
        toolset: 'mch',
        internet: 'no',
        files: 'only if you ask for a teaching sheet',
        does: 'Explains the Massachusetts lead screening schedule (105 CMR 460.000), whether a town is on DPH’s high-risk community list, when to confirm a fingerstick result with a venous test, what to do at each blood lead level (follow-up testing, medical, environmental, and case management steps from DPH’s Childhood Lead Poisoning Prevention Program), reporting rules, the Lead Law, and what to teach families. It can write a family teaching sheet.',
        when: [
          'A home visit to a family with a young child in older housing.',
          'A child has a capillary (fingerstick) result and you need to know how soon to confirm it.',
          'Case management for a child with an elevated or poisoned blood lead level.',
          'Checking whether a 4-year-old in your town still needs a lead test.',
        ],
        provide: [
          'Any of: the child’s age in months, the town, a blood lead level with “capillary” or “venous”, and risk factors (a lead-poisoned sibling, renovation, or peeling paint in a pre-1978 home).',
          'Optionally: ask for a teaching sheet and the family’s language.',
        ],
        examples: [
          'An 18-month-old in Lowell had a fingerstick lead level of 7. What happens next?',
          'What are the follow-up steps for a venous blood lead level of 16 in a 2-year-old?',
          'Is Fitchburg a high-risk community for lead, and what is its screening rate?',
          'Write a lead teaching sheet for a Spanish-speaking family.',
        ],
        returns: [
          'The screenings the child needs and whether each is due, the town’s high-risk status and screening figures, the confirmation timing and actions for the result, reporting duties, Lead Law points, family teaching points, and contacts.',
          'If you asked for one, a teaching sheet in your output folder.',
        ],
        notes: [
          'The high-risk list changes every year; this version uses DPH’s 2024 list (17 communities).',
          'DPH’s follow-up table mixes Massachusetts rules (lead poisoning at 10 µg/dL, level of concern 5 to 9) with CDC’s 3.5 µg/dL reference value. Follow the child’s provider and CLPPP.',
          'The server ships an English teaching sheet. To give families your agency’s translation, save it as templates/<language>/lead_family_teaching.docx in your data folder; otherwise the English sheet is used and the answer says so.',
          'For a language with two words, name the folder haitian_creole (recommended), haitian-creole, or ht.',
        ],
      },
      {
        id: 'mandated_reporter_51a_guide',
        title: 'Mandated reporter (51A) guide',
        toolset: 'mch',
        internet: 'no',
        files: 'only when drafting the written report from your notes',
        does: 'Explains Massachusetts child abuse and neglect reporting under M.G.L. c. 119, §51A: who must report, the immediate call to DCF and the written report within 48 hours, what to include, protections and penalties, DCF’s definitions, domestic violence and substance-exposure questions, and what DCF does next. It gives you the written-report template and can fill a draft from notes you keep in your data folder, without sending those notes to the AI.',
        when: [
          'You are worried about a child after a home visit and want to know your duty and the steps.',
          'You called DCF and need to file the written report within 48 hours.',
          'Training new staff on mandated reporting.',
        ],
        provide: [
          'Optionally: the kind of concern (for example neglect or domestic violence; no names), whether you work for an agency with a person in charge, and the time you called DCF.',
          'To draft the written report: the name of a notes file in your data folder (a text file with “Field name: value” lines, or a spreadsheet with one row per report).',
        ],
        examples: [
          'Am I required to report this, and how? I saw unexplained bruises on a toddler during a home visit.',
          'I called DCF at 3:20 pm today. When is my written 51A report due, and what goes in it?',
          'Give me the 51A written report template.',
          'Draft the written report from 51a_notes.txt in my data folder.',
        ],
        returns: [
          'The steps, the deadline for the written report, guidance on each topic with citations, notes for the kind of concern, and the template’s sections and field names.',
          'When drafting: a Word draft in your output folder for each report, with only the file name and how many fields were filled shown in the chat.',
        ],
        notes: [
          'Never type the family’s names or details into the chat; keep them in a notes file in your data folder.',
          'Unknown entries are marked “?” in the draft, as DCF’s form asks. Review the draft, then submit it through DCF’s online form or DCF’s reporting form.',
          'The statute says prenatal substance exposure alone does not require a report, but DCF’s guide still lists newborn drug dependence; check current DCF practice.',
          'This is guidance, not legal advice. When unsure, call DCF.',
        ],
      },
      {
        id: 'check_developmental_milestones',
        title: 'Check developmental milestones',
        toolset: 'mch',
        internet: 'no',
        files: 'no',
        does: 'Lists CDC’s “Learn the Signs. Act Early.” milestones for a child’s age (2 months to 5 years), lets you mark the ones the child does not yet do, and, if any are missed or skills were lost, gives the act-early steps: talk with the doctor, developmental screening, and a Massachusetts Early Intervention referral (or preschool special education from age 3), with the EI programs that serve the family’s town.',
        when: [
          'A home visit or well-child check where you go over development with the family.',
          'A parent is worried about speech, movement, or behavior.',
          'You need the Early Intervention program for a family’s town.',
        ],
        provide: [
          'The child’s age in months. Optionally: weeks of gestation if born early, the IDs of milestones not yet seen (from the checklist the tool gives you), lost skills or other concerns, and the town.',
        ],
        examples: [
          'Show the milestones for a 15-month-old.',
          'This 18-month-old does not say three or more words (18m-language-1). What next? The family lives in Brockton.',
          'A 6-month-old born at 30 weeks: which milestones should we look for?',
        ],
        returns: [
          'The checklist with an ID for each milestone, whether to act early and how, screening that is due, Early Intervention eligibility and referral, the EI programs for the town with phone numbers, and questions to share with the doctor.',
        ],
        notes: [
          'Milestones are what most children (75% or more) do by an age. The checklists do not replace standardized developmental screening.',
          'For babies born early, the tool also shows the corrected-age checklist; decide with the child’s provider which to use.',
          'Anyone can refer a child to Early Intervention, at no cost to families.',
          'For a baby born early, milestones marked “not yet” on the corrected-age checklist also count toward acting early.',
        ],
      },
      {
        id: 'plan_newborn_postpartum_visit',
        title: 'Plan a newborn and postpartum visit',
        toolset: 'mch',
        internet: 'no',
        files: 'only if you ask for documents',
        does: 'Builds a home visit plan for a newborn and the birthing parent: what to check and what to teach about feeding and weight, jaundice, safe sleep, car seats, Massachusetts newborn blood spot and hearing screening, and CDC’s urgent maternal warning signs. It can save a visit plan and a family teaching sheet as Word documents.',
        when: [
          'Preparing for a postpartum or newborn home visit.',
          'You need a teaching sheet to leave with the family.',
          'Checking that newborn screening follow-up is complete.',
        ],
        provide: [
          'The baby’s age in days. Optionally: how the baby is fed, birth weight and today’s weight, whether the baby went home within 24 hours, the hearing screen result, and whether you want documents (and the family’s language).',
        ],
        examples: [
          'Plan a home visit for a 5-day-old breastfed baby; birth weight 7 lb 8 oz, today 6 lb 15 oz.',
          'The baby did not pass the hearing screen and went home at 20 hours. What should I follow up on?',
          'Make a visit plan and a teaching sheet for a family that speaks Portuguese.',
        ],
        returns: [
          'Follow-up items from what you entered, a checklist and teaching points for each topic with citations, the urgent maternal warning signs, and the documents written to your output folder.',
        ],
        notes: [
          'The weight change is reported, not judged: share it with the baby’s provider.',
          'The jaundice points are general (low confidence) because the full AAP guideline could not be retrieved; follow the baby’s discharge plan.',
          'Documents contain no information about the family. Translations come from your agency’s own files (templates/<language>/newborn_postpartum_teaching.docx); otherwise the English sheet is used.',
        ],
      },
      {
        id: 'estimate_benefit_eligibility',
        title: 'Screen for benefit eligibility',
        toolset: 'equity',
        internet: 'no',
        files: 'no',
        does: 'Compares a household’s size and income with the current income limits for WIC, SNAP, MassHealth, the Health Safety Net, and home heating assistance, and shows the percent of the federal poverty guideline. It is a screen to help you encourage families to apply, not an eligibility decision.',
        when: [
          'A home visit where the family is struggling with food, health coverage, or heating costs.',
          'Deciding which applications to help a family start.',
        ],
        provide: [
          'The number of people in the household and the gross income (yearly or monthly). Optionally: pregnancy, a baby or young children, children under 19, a young adult 19 or 20, whether the family already gets SNAP, MassHealth, or TAFDC, and whether they pay for heat.',
        ],
        examples: [
          'A family of 3 with a newborn earns $3,500 a month. Which programs might they qualify for?',
          'A pregnant woman living alone earns $40,000 a year. Is she under the MassHealth limit?',
        ],
        returns: [
          'The percent of the poverty guideline and, for each program, the income limit for the household, whether the income is under it, the tier (for example the Health Safety Net with a deductible), SNAP’s maximum benefit, notes, how to apply, and the date the limits took effect.',
        ],
        notes: [
          'Only income is checked. Programs also look at immigration status, residency, assets, and who counts in the household, so encourage families to apply.',
          'Limits change every year (poverty guidelines in January, MassHealth in March, WIC in June or July, SNAP and heating assistance in October); the answer shows each date.',
          'This tool is in the equity group.',
        ],
      },
      {
        id: 'map_lead_risk',
        title: 'Map lead risk for outreach',
        toolset: 'mch',
        internet: 'no',
        files: 'yes, a map in output/maps',
        does: 'Ranks cities and towns, or census tracts, for childhood lead outreach using old housing, low income, young children, DPH’s lead screening rates and blood lead levels, and DPH’s high-risk community list, and draws a map.',
        when: [
          'Planning lead outreach, screening events, or deleading promotion.',
          'A grant application or board of health presentation on lead.',
          'Finding the neighborhoods in your town with the oldest housing.',
        ],
        provide: [
          'Optionally: towns or tracts, a county or list of towns, which measures to use, and how many areas to list.',
        ],
        examples: [
          'Which towns in Bristol County should we prioritize for lead outreach?',
          'Rank the census tracts in Lowell by pre-1950 housing and low income, and make a map.',
          'Show the 30 Massachusetts towns with the lowest lead screening rates and old housing.',
        ],
        returns: [
          'A ranked table with each measure, the outreach index, DPH’s high-risk status and score, statewide values for comparison, high-risk communities that did not make the list, and the map files.',
        ],
        notes: [
          'The outreach index is the server’s planning ranking (the average of each measure’s percentile rank), not a DPH method.',
          'Housing age comes from the Census Bureau’s survey, which groups homes as built before 1980, not 1978. Tract estimates are less precise.',
          'Screening figures are from DPH’s 2024 report (2023 for 24 communities whose page was missing from it).',
        ],
      },
      {
        id: 'maternal_infant_health_profile',
        title: 'Maternal and infant health profile',
        toolset: 'mch',
        internet: 'no',
        files: 'no',
        does: 'Reports births, low birthweight, preterm birth, teen births, prenatal care, infant mortality, and PRAMS survey results (postpartum depression, breastfeeding, safe sleep, unintended pregnancy) for Massachusetts, a town, or a county, by race and Hispanic ethnicity where DPH publishes it, compared with the state and the U.S.',
        when: [
          'A community health assessment or MCH needs assessment.',
          'Grant applications and presentations on maternal and infant health equity.',
          'Checking how your town compares with the state on low birthweight or preterm birth.',
        ],
        provide: [
          'Optionally: a town or county, and whether to include race and ethnicity and trends.',
        ],
        examples: [
          'Give me a maternal and infant health profile for Springfield.',
          'How does infant mortality in Massachusetts differ by race and ethnicity?',
          'What do PRAMS data show about safe sleep and postpartum depression in Massachusetts?',
        ],
        returns: [
          'For a town or county: births, low birthweight and preterm percentages with 95% confidence intervals and whether they differ from the state, and teen births. Statewide tables by race and ethnicity with ratios to White non-Hispanic, PRAMS results, trends, and U.S. comparisons.',
        ],
        notes: [
          'Town figures come from DPH’s 2023 birth report; small counts are suppressed and percentages from fewer than 20 births are unstable.',
          'Infant mortality and PRAMS are statewide only (DPH does not publish them by town).',
          'U.S. figures are provisional CDC estimates for slightly different years.',
        ],
      },
    ],
  },
  // End of Phase 8 chapter.
  // Phase 9 chapter goes below this line.
  {
    title: 'Public health nursing practice and case management',
    intro: 'These tools support everyday case management and documentation: TB contact investigations and latent TB treatment, social needs screening and referral, community resources, newcomer health, Omaha System coding, the Public Health Intervention Wheel, and documentation templates. The TB tools are in the cd group; social needs, resources, and newcomer health are in the equity group; the Omaha System, Intervention Wheel, and documentation tools are in the core group, which is always on. Notes and client lists are read in your data folder and never sent back to the assistant; only codes, counts, and blank templates come back. All of the reference content is a draft awaiting your review.',
    tools: [
      {
        id: 'plan_tb_contact_investigation',
        title: 'Plan a TB contact investigation',
        toolset: 'cd',
        internet: 'no',
        files: 'optional',
        does: 'Applies the 2005 NTCA/CDC contact investigation guidelines to an index case: whether to investigate, the infectious period and the window to ask about, contact priority (high, medium, low) from the published decision trees, deadlines for the first encounter and the medical evaluation in business days, and the testing schedule with the retest 8–10 weeks after last exposure. It lists the DPH TB program numbers, reporting rules, and state TB clinics, and writes a tracking workbook with formulas.',
        when: [
          'A new case of pulmonary TB has been reported in your town.',
          'Deciding which contacts to test first and when to retest.',
          'Checking whether test results call for expanding the investigation.',
        ],
        provide: [
          'The case’s clinical facts: symptoms, smear and chest X-ray results, and dates (symptom onset, first positive finding, treatment start).',
          'Contact groups as counts and risk facts (setting, age band, HIV or immunosuppression), never names. Or a contact list in your data folder.',
        ],
        examples: [
          'Plan a TB contact investigation for a smear-positive cavitary case with cough since June 10: 4 household adults, 6 children under 5 at a child care center, and 20 coworkers.',
          'Use tb_contacts.csv to assign contact priorities and tell me whether we should expand.',
        ],
        returns: [
          'The infectious period with the rule used, each group’s priority and why, first-encounter deadlines, retest windows, actions for young children and immunocompromised contacts, DPH contacts and clinics, and the workbook in output/.',
          'With a contact list: counts by priority, tested and positive counts, and expansion signals only.',
        ],
        notes: [
          'Duration limits for “high” and “medium” exposure are set by your program; there is no national cut-off.',
          'Business days skip weekends but not holidays.',
          'If you give a symptom onset date, the index patient is treated as having symptoms. In a contact file, ages such as “8 months” are read; contacts whose age cannot be read are counted, because the age rules were not applied to them.',
        ],
      },
      {
        id: 'ltbi_treatment_calendar',
        title: 'Latent TB treatment calendar',
        toolset: 'cd',
        internet: 'no',
        files: 'optional',
        does: 'Builds a dose calendar and adherence tracker for a latent TB infection regimen (3HP, 4R, 3HR, 6H, 9H, or twice-weekly isoniazid): every dose date, monitoring visits, and the completion window, such as 11 of 12 weekly 3HP doses within 16 weeks. Given the dates doses were taken, it counts valid doses, flags gaps of more than 2 months, and says whether completion still fits the window. It never calculates doses.',
        when: [
          'Starting a client on latent TB treatment.',
          'Checking whether a client who missed doses can still finish in time.',
          'Preparing a calendar for a client or a DOT schedule.',
        ],
        provide: [
          'The regimen and the first dose date; optionally DOT or self-administered, dose dates taken, and the client’s language.',
        ],
        examples: [
          'Make a 4R calendar starting January 15 in Portuguese.',
          'My client on 3HP took doses January 5, 12, and 19 and nothing since. It is March 30. Can she still complete?',
        ],
        returns: [
          'The schedule, visits, completion deadline, CDC and DPH positions, an adherence status, an XLSX tracker, and a DOCX patient calendar in output/.',
        ],
        notes: [
          'Patient instructions come from your agency’s translation in templates/<language>/ltbi_calendar.md when you provide one; otherwise English with a reminder to use an interpreter.',
          'DPH’s page lists 3HP, 4R, and 9H; CDC also lists 3HR and 6H.',
        ],
      },
      {
        id: 'screen_health_related_social_needs',
        title: 'Screen health-related social needs',
        toolset: 'equity',
        internet: 'no',
        files: 'optional',
        does: 'Scores the CMS Accountable Health Communities social needs screening tool by domain (housing, food, transportation, utilities, safety, and the supplemental domains) and maps each positive domain to Massachusetts referral options such as Emergency Assistance shelter, Mass 211, SNAP, WIC, fuel assistance, and MassHealth ACO social needs services. It can also summarize a file of many screenings.',
        when: [
          'After a client completes the screening at a home visit or clinic.',
          'Summarizing screenings for a program report.',
        ],
        provide: [
          'The answers by question number or item name (no names), and the client’s age for the physical activity question; or a file of screenings in your data folder.',
        ],
        examples: [
          'Score this AHC screen: Q1 not steady, Q3 often true, Q6 already shut off, safety items 1, 2, 1, 1.',
          'Summarize hrsn_screenings.csv by site.',
        ],
        returns: [
          'Each domain as positive, negative, incomplete, or not screened with the reason, and referral options for positive domains; for a file, counts by domain with small numbers suppressed and per-person results in output/.',
        ],
        notes: [
          'A positive domain means the person might have an unmet need; talk with them before referring.',
          'Exact question wording is shown only for public-domain items; use the CMS form for the rest.',
          'When you summarize by site or group, small counts are hidden across all the groups together, so a hidden count cannot be found by subtracting the other groups from the total.',
        ],
      },
      {
        id: 'omaha_system_coder',
        title: 'Code a note with the Omaha System',
        toolset: 'core',
        internet: 'no',
        files: 'optional',
        does: 'Reads a visit narrative or care plan and suggests Omaha System problems (with domain, modifiers, and signs and symptoms), interventions as category and target, and the Knowledge-Behavior-Status rating scale. It can label your ratings and show change from baseline, look up a problem or target, and write a blank care plan. The note is never sent back.',
        when: [
          'Documenting a home visit in Omaha System terms.',
          'Teaching or checking Omaha System coding.',
        ],
        provide: [
          'The note (as text, or better, a file in your data folder), a term to look up, or ratings to label.',
        ],
        examples: [
          'Code my visit note visit_2026-11-05.docx with the Omaha System and draft a care plan.',
          'Knowledge 3, Behavior 2, Status 2 for Medication regimen; admission was 2, 2, 1.',
        ],
        returns: [
          'Problem codes and names with the matched terms, suggested interventions, the rating scale, and the care plan file in output/.',
        ],
        notes: [
          'Suggestions come from keywords; confirm every problem and intervention.',
          'Signs and symptoms marked unverified are drafts awaiting your check against the 2005 Omaha System book.',
          'Problems mentioned only with a negative (“no income”, “not taking insulin”, “denies fever”) are listed separately for you to decide: an absent need is often the problem itself.',
        ],
      },
      {
        id: 'classify_intervention_wheel',
        title: 'Classify activities by the Intervention Wheel',
        toolset: 'core',
        internet: 'no',
        files: 'optional',
        does: 'Classifies public health nursing activities by the 17 interventions, 5 wedges, and 3 levels of practice of the Minnesota Public Health Intervention Wheel, for reports and workload analysis. It can classify an activity log in your data folder and total the hours by intervention.',
        when: [
          'Annual reports and workload analysis.',
          'Teaching students or new staff about population-based practice.',
        ],
        provide: [
          'Short activity descriptions, or an activity log with a description column (and optionally hours and a program column).',
        ],
        examples: [
          'Classify: drafted a tobacco regulation; flu clinic at the senior center; TB contact tracing.',
          'Classify activity_log.xlsx by intervention and level, with hours by program.',
        ],
        returns: [
          'The intervention, wedge, and level for each activity (by number), or counts and hours by intervention and level, with each activity’s result in output/.',
        ],
        notes: [
          'Four definitions and the level definitions could not be checked against MDH’s own page and are marked low confidence.',
        ],
      },
      {
        id: 'nursing_documentation_template',
        title: 'Nursing documentation template',
        toolset: 'core',
        internet: 'no',
        files: 'optional',
        does: 'Gives SOAP, DAR (focus charting), or narrative templates for home visits, clinic visits, communicable disease case investigations, and DOT visits, with the required elements to include (date and time, signature with credentials, consent, language and interpreter, teaching with teach-back, response, follow-up, and visit-specific items). It can check a note in your data folder for those elements without sending the note back.',
        when: [
          'Setting up or reviewing documentation for a program.',
          'Checking your own note before signing it.',
        ],
        provide: [
          'The format and visit type; optionally a note file to check, and elements your agency documents elsewhere.',
        ],
        examples: [
          'Give me a DAR template for a DOT visit.',
          'Check my note home_visit_note.docx for required elements.',
        ],
        returns: [
          'The template as Markdown (and DOCX in output/ if asked), and for a note, which elements were found and in which section.',
        ],
        notes: [
          'The structures are common defaults; follow your agency’s documentation policy where it differs.',
          '244 CMR 9.03 applies to every nurse licensed in Massachusetts; 42 CFR 484 is cited only as a reference standard.',
        ],
      },
      {
        id: 'find_community_resources',
        title: 'Find community resources',
        toolset: 'equity',
        internet: 'optional',
        files: 'optional',
        does: 'Searches your agency’s own resource directory (a spreadsheet in your data folder) by town, service, language, and opening hours, together with HRSA-funded community health center sites, same town first, then the rest of the county. Entries not verified in the past year are flagged.',
        when: [
          'Finding a referral during or after a visit.',
          'Keeping your resource list in one place for the whole team.',
        ],
        provide: [
          'The town (or your configured town), the service, and optionally a language and a day and time.',
          'Your directory at resources/community_resources.csv with columns name, services, town, address, phone, website, languages, hours, eligibility, cost, last_verified.',
        ],
        examples: [
          'Find a food pantry near Amherst with Spanish-speaking staff open Saturday morning.',
          'Which community health centers in Worcester offer dental care?',
        ],
        returns: [
          'Matching resources with services, town, phone, languages, hours, and source, plus the directory format if yours needs fixing.',
        ],
        notes: [
          'HRSA lists health center sites, not their services, languages, or daily hours; call to confirm.',
          'A file that looks like a client list is refused.',
          'Health centers listed under a village or neighborhood are placed in their town, for example “Dorchester (Boston)”.',
        ],
      },
      {
        id: 'newcomer_health_navigator',
        title: 'Newcomer health navigator',
        toolset: 'equity',
        internet: 'no',
        files: 'optional',
        does: 'Brings together refugee and immigrant health information for Massachusetts: the CDC domestic medical screening components with age rules and the 90-day window, DPH and Office for Refugees and Immigrants programs, MassHealth coverage by immigration status (including the October 1, 2026 federal changes), and Emergency Assistance family shelter and basic-needs resources.',
        when: [
          'A newly arrived family is referred to your program or living in a shelter in your town.',
          'Questions about what coverage a client may have.',
        ],
        provide: [
          'Topics, and optionally the person’s immigration status, age, pregnancy, disability, arrival date, and preferred language (no names).',
        ],
        examples: [
          'An asylee, 45, arrived September 1. What screening is due, and what MassHealth coverage applies now?',
          'What shelter and food resources are there for a newly arrived family with children?',
        ],
        returns: [
          'Screening components and deadlines, programs with contacts, the likely coverage path with the rules, shelter resources, and a blank checklist in output/ if asked.',
        ],
        notes: [
          'Benefit rules for immigrants changed in 2025–2026 and are still changing; MassHealth decides eligibility. Refer status questions to an immigration legal service on ORI’s list.',
        ],
      },
    ],
  },
  // End of Phase 9 chapter.
  // Phase 10 chapter goes below this line.
  {
    title: 'Healthy aging and mental health screening',
    intro: 'These tools support work with older adults and mental health screening: scoring free screening instruments with safety alerts, finding crisis and aging services by town, fall risk assessment, elder protective services reporting, coverage options, nursing home quality, mapping older adults at risk, and advance care planning. The screening and crisis tools are in the bh group (school_nurse profile); the others are in the aging group. All content was drafted in November 2026 from the cited sources and is pending review.',
    tools: [
      {
        id: 'score_mental_health_screen',
        title: 'Score a mental health screen',
        toolset: 'bh',
        internet: 'no',
        files: 'only for a file of screens',
        does: 'Scores the PHQ-2, PHQ-9, GAD-2, GAD-7, Edinburgh Postnatal Depression Scale, NIMH Ask Suicide-Screening Questions (ASQ), or the C-SSRS screener from the item answers, checks the answers, gives the total and the published cutoff, and raises safety alerts with the steps to take. It never diagnoses.',
        when: [
          'After a depression, anxiety, or perinatal screen at a clinic or home visit.',
          'When a suicide risk screen is positive and you need the next steps.',
          'Summarizing a screening event from a spreadsheet.',
        ],
        provide: [
          'The instrument and the answers in order (numbers, yes/no, or words such as “several days”).',
          'For the ASQ: age, whether the person refused, and when a past attempt was. For the C-SSRS: whether behavior was in the past 3 months.',
          'Or a spreadsheet in your data folder with columns q1, q2, ... (one row per screen).',
        ],
        examples: [
          'Score a PHQ-9: 2, 2, 1, 2, 1, 1, 1, 0, 1.',
          'ASQ answers: no, no, yes, no, and yes to question 5. What now?',
          'Score the EPDS screens in postpartum_screens.xlsx.',
        ],
        returns: [
          'The total, severity, result (for example acute positive), each item, safety alerts with cited steps, 988 and the Crisis Text Line, and the instrument’s license terms. For a file: counts only, with a results file listing rows with alerts first.',
        ],
        notes: [
          'Any answer above zero on PHQ-9 item 9 or EPDS item 10 raises a same-day safety alert, whatever the total.',
          'Only the public-domain ASQ’s wording is included. PHQ, GAD, EPDS, and C-SSRS items are listed by topic; use the official forms.',
          'Follow your agency’s suicide risk protocol.',
          'If PHQ-9 item 9 or EPDS item 10 is not answered, the answer says to ask it before the person leaves: suicide risk has not been screened until it is answered.',
          'A C-SSRS screener with a yes answer but unanswered items is “risk level not yet determined”; follow at least the same-day steps until the items are asked.',
        ],
      },
      {
        id: 'find_crisis_resources',
        title: 'Find crisis resources',
        toolset: 'bh',
        internet: 'no',
        files: 'no',
        does: 'Lists 911, the 988 Lifeline, the Massachusetts Behavioral Health Help Line, Mobile Crisis Intervention for youth and adults, the Crisis Text Line, and the town’s Community Behavioral Health Center, with how to call, text, or chat and language options.',
        when: [
          'Someone is in crisis, or you are preparing a resource card.',
          'A family needs help in Spanish, another language, or ASL.',
        ],
        provide: ['A town (or your saved town), and optionally youth or adult and a language.'],
        examples: [
          'Crisis resources for a teenager in Lowell; the family speaks Spanish.',
          'Who is the Community Behavioral Health Center for Gardner?',
        ],
        returns: ['Which line to use when, each line’s numbers and language options, and the local CBHC’s address and phone.'],
        notes: [
          'Numbers were read from mass.gov and NIMH in November 2026; call-test them before publishing.',
          'No peer warmline is listed because none could be verified.',
        ],
      },
      {
        id: 'steadi_fall_risk_assessment',
        title: 'Assess fall risk (CDC STEADI)',
        toolset: 'aging',
        internet: 'no',
        files: 'only if you ask for a plan document',
        does: 'Scores CDC’s Stay Independent checklist or the three key questions, interprets the Timed Up and Go, 30-Second Chair Stand (by age and sex), 4-Stage Balance Test, and orthostatic blood pressure, and builds an intervention and home-safety plan.',
        when: [
          'A fall risk screen at a home visit, senior center, or clinic.',
          'Planning a fall prevention referral after a fall.',
        ],
        provide: [
          'The 12 Stay Independent answers or the three key questions.',
          'Any test results, age and sex (for the chair stand), and home hazards you saw.',
        ],
        examples: [
          'Stay Independent: yes to items 1, 3, 4, 6, and 10. TUG 14 seconds, 7 chair stands, man aged 82.',
          'Make a fall prevention plan in Word for that assessment.',
        ],
        returns: ['The screen, each test against CDC’s threshold, risk factors, interventions, assessments still to do, home-safety fixes, and follow-up timing.'],
        notes: ['Do not type names; add them to the plan document on your computer.'],
      },
      {
        id: 'elder_protective_services_guide',
        title: 'Report suspected elder or disabled adult abuse',
        toolset: 'aging',
        internet: 'no',
        files: 'only to fill report narratives',
        does: 'Explains Massachusetts reporting rules for abuse, neglect, financial exploitation, and self-neglect of adults 60 and older (Adult Protective Services) and adults 18 to 59 with disabilities (DPPC): who must report, how, how fast, what a report contains, and what happens next. Fills report narratives from a file on your computer.',
        when: [
          'You suspect abuse, neglect, or self-neglect during a visit.',
          'Training staff on mandated reporting.',
          'A count of one or more falls in the past year counts as a fall (at risk).',
        ],
        provide: [
          'Age, setting (community or facility), the concern, and your role. Never names in the chat.',
          'Optionally a spreadsheet in your data folder with the report details (one row per report).',
        ],
        examples: [
          'I am a public health nurse. An 84-year-old client says her son takes her checks. What do I do?',
          'Fill report narratives from apsreport.csv.',
        ],
        returns: ['The agency and hotline, mandated reporter duties, timelines, signs, self-neglect guidance, the narrative template, and counts of filled narratives.'],
        notes: [
          'A phone report by a mandated reporter needs the written form within 48 hours; an online report does not.',
          'Legal decision support, not legal advice.',
          'The answer says you are a mandated reporter only when your role matches the statutory list for that pathway. If it cannot match your role, it says so and shows the list; that does not mean you are not a mandated reporter.',
        ],
      },
      {
        id: 'find_ma_aging_services',
        title: 'Find aging services for a town',
        toolset: 'aging',
        internet: 'no',
        files: 'no',
        does: 'Lists the town’s Aging Services Access Point, Council on Aging, SHINE Medicare counseling office, senior nutrition (meals) project, caregiver and dementia supports, and protective services and ombudsman contacts.',
        when: ['Connecting an older adult or caregiver to services.', 'Building a resource list for a town.'],
        provide: ['A town, and optionally needs such as meals, insurance, caregiver, or dementia.'],
        examples: ['What aging services are there for Pittsfield?', 'Who delivers meals and does SHINE counseling in Orange?'],
        returns: ['Providers with addresses and phones, statewide lines, and what each program offers.'],
        notes: ['Council on Aging phone numbers are not included; call MassOptions (800) 243-4636 or use AGE’s map.'],
      },
      {
        id: 'explain_older_adult_coverage_options',
        title: 'Explain coverage options for older adults',
        toolset: 'aging',
        internet: 'no',
        files: 'no',
        does: 'Explains Medicare (with this year’s premiums and deductibles), Medicare Savings Programs, MassHealth for people 65 and older, the Frail Elder Waiver, PACE, Senior Care Options, and One Care, and screens each from the facts you give.',
        when: ['An older adult cannot afford Medicare costs.', 'Exploring options to stay at home with care needs.'],
        provide: ['Any of: age, household size, monthly income, assets, current coverage, nursing home level of care, and town.'],
        examples: ['A 70-year-old couple with $3,900 a month and Medicare: what help is there?', 'Is PACE available in Barnstable?'],
        returns: ['Medicare costs, each program’s rules and a screen with reasons, the Medicare Savings Program level, PACE availability, and where to get help.'],
        notes: ['A screen, not an eligibility decision. Medicare figures are for 2026; update each November.'],
      },
      {
        id: 'nursing_home_quality_lookup',
        title: 'Look up nursing home quality',
        toolset: 'aging',
        internet: 'optional',
        files: 'no',
        does: 'Shows CMS Care Compare data for Massachusetts nursing homes: star ratings, staffing, inspections and infection control citations, penalties, and staff and resident vaccination, with state and national averages.',
        when: ['Preparing for an outbreak response or infection control visit.', 'Comparing facilities in your area.'],
        provide: ['A facility name or CMS number, a town or county, or ask for a statewide summary.'],
        examples: ['Infection control citations for nursing homes in Worcester.', 'Statewide, which nursing homes have the lowest staff flu vaccination?'],
        returns: ['Ratings, staffing, citations with F-tags and severity, vaccination rates, flags, and averages.'],
        notes: ['Citations may be under dispute or already corrected; check each one’s status.'],
      },
      {
        id: 'map_older_adult_vulnerability',
        title: 'Map older adults at risk',
        toolset: 'aging',
        internet: 'optional',
        files: 'only for a map',
        does: 'Shows where older adults who live alone, have a disability, have no vehicle, speak limited English, or live in poverty are concentrated, by census tract or town, with margins of error and a map.',
        when: ['Planning outreach or wellness checks.', 'Emergency preparedness and grant narratives.'],
        provide: ['A town, county, or Massachusetts, and optionally what to rank by and whether to draw a map.'],
        examples: ['Which tracts in Springfield have the most older adults living alone? Make a map.', 'Rank Franklin County towns by older adults without a vehicle.'],
        returns: ['The area’s five indicators compared with the state and U.S., ranked sub-areas, reliability flags, and a map file.'],
        notes: ['Small-area survey estimates can be unreliable; look at the margins of error.'],
      },
      {
        id: 'advance_care_planning_toolkit',
        title: 'Advance care planning toolkit',
        toolset: 'aging',
        internet: 'no',
        files: 'for session materials and filled proxies',
        does: 'Explains the Massachusetts health care proxy, provides a proxy template, a facilitator script and handout for a community session, and the status of MOLST, and fills proxy forms from a file on your computer, using your agency’s translations when you have them.',
        when: ['Running a community advance care planning session.', 'Helping clients complete health care proxies.'],
        provide: ['Session length and whether you want Word or PDF materials; optionally a spreadsheet with proxy details and a Language column.'],
        examples: ['Plan a 45-minute health care proxy session at the senior center and make the handouts.', 'Fill health care proxies from proxy_clinic.csv.'],
        returns: ['Proxy rules from the law, the template, the script with timings, the handout, MOLST status, and counts of filled forms by language.'],
        notes: [
          'MOLST could not be verified: the old molst-ma.org web address no longer belongs to the program. Get the current form from your health system or DPH.',
          'Put agency translations in templates/<language>/health_care_proxy.md in your data folder.',
          'Nursing homes that CMS lists under a village or neighborhood (Hyannis, Dorchester) are matched to their town by ZIP code; the answer says when, because a ZIP code can cross a town line.',
        ],
      },
    ],
  },
  // End of Phase 10 chapter.
  // Phase 11 chapter goes below this line.
  {
    title: 'Substance use, crisis resources, and health communication',
    intro: 'These tools support overdose prevention, youth mental health, school crisis response, and the materials you write for the public. The substance use and postvention tools are in the bh group (the school_nurse profile includes it; turn it on for other profiles); the communication tools are in the comms group, which every profile includes. Resource lists, safe-messaging rules, and templates were drafted from DPH, CDC, SAMHSA, AFSP, and SPRC documents retrieved in November 2026 and are pending review by the subject matter expert; every answer says so. None of these tools replaces a crisis line: in an emergency, call 911, and for a mental health or substance use crisis, call or text 988.',
    tools: [
      {
        id: 'check_readability',
        title: 'Check the reading level and plain language of a draft',
        toolset: 'comms',
        internet: 'no',
        files: 'optional: a Word, HTML, PDF, text, or Markdown file in your data folder',
        does: 'Scores a draft with four reading-level formulas (Flesch Reading Ease, Flesch-Kincaid, SMOG, and Gunning Fog), flags long sentences, public health jargon (with CDC’s *Everyday Words* alternatives), possible passive voice, acronyms that are not spelled out, and long paragraphs and lists, and walks you through the CDC Clear Communication Index.',
        when: [
          'Before you publish a fact sheet, flyer, letter, web page, or social media post.',
          'Revising a material that families or clients find hard to read.',
          'Scoring a material with the CDC Clear Communication Index for a grant or quality project.',
        ],
        provide: [
          'The text, pasted into the chat, or the name of the file in your data folder.',
          'Optionally the grade level you aim for (the tool uses grade 8 unless you say otherwise) and your yes or no answers to the 20 Clear Communication Index questions.',
        ],
        examples: [
          'Check the reading level of this flyer and suggest plainer words: [paste text].',
          'Check lead_poisoning_factsheet.docx against a 6th-grade target.',
          'Score this material with the CDC Clear Communication Index. Here are my answers: 1 yes, 2 yes, 3 no ...',
        ],
        returns: [
          'The grade level by each formula and the median compared with your target; counts of words, sentences, and syllables; the flags with suggested fixes; and the Clear Communication Index items with the tool’s suggestions, or your score (90 or higher passes) once you have answered every item that applies.',
        ],
        notes: [
          'Reading-level formulas only count syllables and sentence length. They cannot tell whether your audience will understand or act on a material; test drafts with members of your audience.',
          'The formulas and word list are for English. For other languages, rely on a qualified translator.',
          'When you check a file, quoted sentences are passed through the redactor, but remove client details before publishing anything.',
          'Syllables are counted more accurately than in version 0.14.0 (for example safety, diarrhea, influenza), so grades can differ slightly from earlier results.',
        ],
      },
      {
        id: 'get_overdose_data',
        title: 'Look up overdose deaths for your community',
        toolset: 'bh',
        internet: 'optional',
        files: 'no',
        does: 'Shows overdose deaths for a city or town, county, or the state: DPH’s yearly counts of opioid-related overdose deaths by town (2015 to 2022) with a 5-year rate compared with the state; CDC’s provisional counts for the latest 12 months for counties, Massachusetts, and the United States, with the change from a year earlier and a breakdown by drug (fentanyl, heroin, cocaine, stimulants); and DPH’s statewide findings for 2024.',
        when: [
          'Preparing a board of health update, a grant application, or a community health assessment.',
          'Planning naloxone distribution or outreach and deciding where to focus.',
          'Answering a reporter’s or official’s question about local overdose trends.',
        ],
        provide: [
          'The town, county, or “Massachusetts” (your saved town is used if you do not say), and optionally other places to compare and a drug class.',
          'Whether you want deaths of residents (the default) or deaths that happened in the town.',
        ],
        examples: [
          'How many opioid overdose deaths has Pittsfield had, and how does its rate compare with the state?',
          'Compare overdose deaths in Fall River and New Bedford and their county.',
          'What is the latest CDC count of fentanyl deaths in Massachusetts, and is it going down?',
        ],
        returns: [
          'Yearly town counts, the 2018–2022 rate with its 95% confidence interval and how it compares with the state, county and state trends with the percent change, counts by drug, DPH’s 2024 findings with page numbers, and a link to DPH’s dashboard for newer town data.',
        ],
        notes: [
          'DPH last published town counts as a document for 2015–2022. Newer town numbers (through 2025) are on DPH’s BSAS dashboard; the tool gives the link.',
          'No rate is shown for fewer than 5 deaths, and rates based on fewer than 20 deaths are marked unstable. CDC hides county counts of 1 to 9.',
          'DPH, CDC state, and CDC county numbers use different definitions (opioid-related deaths by residence; all overdose deaths by place of death; all overdose deaths by county of residence), so they do not add up to each other.',
        ],
      },
      {
        id: 'get_youth_risk_behavior_data',
        title: 'Look up youth mental health and risk behavior data',
        toolset: 'bh',
        internet: 'optional',
        files: 'no',
        does: 'Shows how many Massachusetts middle and high school students report persistent sadness, thinking about suicide, self-injury, bullying, vaping, drinking, marijuana use, protective factors such as feeling they belong at school, and other topics, from DPH’s Youth Health Survey (2023 and 2025) and the Massachusetts Youth Risk Behavior Survey (2023), by sex, grade, race and ethnicity, and LGBTQ identity, with 95% confidence intervals. Compares Massachusetts with the United States using CDC’s Youth Risk Behavior Survey.',
        when: [
          'Making the case for a school-based mental health or substance use program.',
          'A community health assessment or a grant application section on youth.',
          'Preparing a presentation for a school committee or parent group.',
        ],
        provide: [
          'The topic (for example “sad or hopeless”, “vaping”, or “bullied”), high school or middle school, and which groups you want to see.',
        ],
        examples: [
          'What percentage of Massachusetts high school students felt sad or hopeless, by sex and race?',
          'How common is vaping among Massachusetts middle school students, by grade?',
          'How does Massachusetts compare with the U.S. on suicidal thoughts among high school students?',
        ],
        returns: [
          'For each matching indicator: the statewide percentage and each group’s percentage with its 95% confidence interval, which groups differ from the comparison group, the change from two years earlier, and CDC figures for Massachusetts and the U.S.',
        ],
        notes: [
          'These are statewide figures for public school students. There are no town or school figures.',
          'A difference is reported only when the confidence intervals do not overlap, DPH’s rule. Estimates DPH or CDC did not publish (too few students) are shown as not presented.',
          'The Youth Health Survey, the Massachusetts YRBS, and CDC’s figures are different surveys; compare years within one survey only.',
          'When you share suicide-related numbers, follow safe-messaging guidance and include the 988 Lifeline.',
        ],
      },
      {
        id: 'find_naloxone_and_harm_reduction',
        title: 'Find naloxone and harm reduction resources',
        toolset: 'bh',
        internet: 'no',
        files: 'only if you ask for a handout',
        does: 'Explains how anyone in Massachusetts can get naloxone (at a pharmacy counter without a prescription under the statewide standing order, from distribution programs, by home delivery, or through agency purchasing), lists the naloxone products in the standing order, says whether your town’s board of health has approved a syringe services program, gives facts about fentanyl test strips and help lines, and provides a step-by-step naloxone teaching script, which it can save as a Word handout.',
        when: [
          'Teaching a client, family, or community group how to respond to an overdose.',
          'Answering “where can I get Narcan?” or “does my insurance cover it?”',
          'Planning naloxone distribution or a syringe services conversation with your board of health.',
        ],
        provide: [
          'Your town (or use your saved town), and whether you want the teaching script and a Word handout.',
        ],
        examples: [
          'How can a family in Pittsfield get naloxone, and is there a syringe services program?',
          'Give me a naloxone teaching script for a parent group and save it as a handout.',
          'What naloxone products does the Massachusetts standing order list?',
        ],
        returns: [
          'Ways to get naloxone, the standing order’s products and directions, pharmacy duties and legal protections, syringe services approval for your town and county, fentanyl test strip facts, help lines, links to DPH’s site maps, and the teaching script (and the handout file name, if requested).',
        ],
        notes: [
          'DPH’s maps of naloxone programs and syringe service sites cannot be read by the server; open the links for current sites and hours.',
          'Whether fentanyl test strips are legal under Massachusetts law could not be confirmed; ask your agency’s counsel.',
          'Rescue breathing and CPR steps are for people trained to do them. Follow your agency’s protocols.',
        ],
      },
      {
        id: 'find_substance_use_treatment',
        title: 'Find substance use treatment',
        toolset: 'bh',
        internet: 'optional',
        files: 'no',
        does: 'Lists substance use treatment programs near a Massachusetts town from SAMHSA’s FindTreatment.gov: programs that offer methadone, buprenorphine, or naltrexone for opioid use disorder, withdrawal management (detox), residential or outpatient care, and recovery supports, and programs for teens, pregnant or postpartum women, veterans, or people with mental health conditions, filtered by insurance (including MassHealth) and language.',
        when: [
          'Helping a client or family find treatment that takes MassHealth.',
          'Finding a methadone or buprenorphine program after an overdose or a hospital stay.',
          'Building a local referral list for staff.',
        ],
        provide: [
          'The town (or use your saved town), how far the person can travel, the kind of treatment, the insurance, and the language needed.',
        ],
        examples: [
          'Find buprenorphine treatment within 10 miles of Fall River that takes MassHealth.',
          'Is there a methadone program near Springfield with Spanish-speaking staff?',
          'List residential programs for pregnant women within 30 miles of Worcester.',
        ],
        returns: [
          'Matching programs nearest first, with address, phone, website, distance, medications offered, settings, insurance accepted, and languages; and the Massachusetts Substance Use Helpline and 988.',
        ],
        notes: [
          'Programs report their own services to SAMHSA. Call ahead to confirm openings, insurance, and language.',
          'Distances are straight-line miles from the center of the town.',
          'The Massachusetts Substance Use Helpline (800-327-5050) can also help find treatment and recovery services.',
          'Facilities outside Massachusetts are flagged: their “Medicaid” is that state’s program, not MassHealth.',
        ],
      },
      {
        id: 'plan_school_postvention',
        title: 'Plan a school’s response after a suicide',
        toolset: 'bh',
        internet: 'no',
        files: 'optional: a file with the school’s notice details, to fill the letters',
        does: 'Walks a school crisis team through the steps in the AFSP and SPRC toolkit *After a Suicide: A Toolkit for Schools* for the first day, the first week, and after; gives talking points; provides draft messages for the staff meeting, students, parents, the media, and social media that fit what is known about the cause of death; checks your own drafts against safe-messaging rules; and fills the letters from a file in your data folder.',
        when: [
          'A student or staff member has died and suicide is suspected or confirmed.',
          'Reviewing a letter, statement, or post about a suicide before it goes out.',
          'Preparing a postvention plan before a crisis happens.',
        ],
        provide: [
          'Whether the cause of death is confirmed, unconfirmed, or the family asked that it not be shared, and which messages you need.',
          'Optionally a draft to check, and a spreadsheet in your data folder with columns such as School Name, Student Name, Principal Name, Date of Notice, and Counseling Location to fill the letters. Do not type names into the chat.',
        ],
        examples: [
          'A student died over the weekend and the cause is not confirmed. What should our school do today, and what do we tell students?',
          'Check this draft letter to parents for safe messaging: [paste text].',
          'Fill the parent letter and student statement from school_notice.xlsx.',
        ],
        returns: [
          'Steps by phase with toolkit page numbers, talking points, the message templates with their placeholders, any safe-messaging problems in your draft (by sentence number), and the names of the filled Word files in your output folder.',
        ],
        notes: [
          'The templates are drafts written for this server. The toolkit’s own sample statements can be used directly from the PDF with its credit line.',
          'The family’s wishes and your crisis team decide what is shared. Confirm the cause of death before calling it a suicide.',
          'The tool never describes how a person died. The safe-messaging check uses word patterns; a person must still review every message.',
          'Anyone in crisis can call or text 988.',
        ],
      },
      {
        id: 'translate_health_material',
        title: 'Prepare and check a translation',
        toolset: 'comms',
        internet: 'optional',
        files: 'optional: the English source, your glossary, a translated draft, a back-translation',
        does: 'Supports translating a health material without replacing a qualified translator: shows which languages are most needed in your town, finds your agency’s approved translations, prepares a brief for the translator (numbered sentences, your glossary’s approved terms, and numbers, phone numbers, and web addresses that must not change), checks a translated draft against your glossary, and compares a back-translation with the English to catch reversed instructions, changed numbers, and missing sentences.',
        when: [
          'Before sending a flyer or letter to a translator.',
          'Reviewing a draft translation (including one the AI assistant wrote) before a qualified translator approves it.',
          'Deciding which languages to translate a material into.',
        ],
        provide: [
          'The English text or file, the language, and optionally your glossary file, the translated draft, and a back-translation by a different person.',
          'Keep approved translations in your data folder under templates/<language>/ (for example templates/spanish/flu_flyer.docx) so the tool can find them.',
        ],
        examples: [
          'Which languages should our flu flyer be translated into for Lawrence?',
          'Prepare a translator brief for this flyer in Portuguese using glossary.csv.',
          'Compare this Spanish back-translation with the English and tell me what changed.',
        ],
        returns: [
          'Language priorities, any existing agency translation, the numbered sentences, glossary terms, items that must not change, problems found in the draft and back-translation, CDC’s translation steps, the “draft for qualified translator review” label, and an optional review workbook.',
        ],
        notes: [
          'The server does not include or produce translations. Every draft must be reviewed and approved by a qualified translator.',
          'The checks look at numbers, phone numbers, glossary terms, negations, and missing sentences; they cannot judge meaning or tone.',
          'Numbers such as 3.5, 1,000, and 9:30 are checked whole, and a translation that writes them the local way (1.000 or 3,5) still matches.',
        ],
      },
      {
        id: 'create_social_media_campaign',
        title: 'Plan and check a social media campaign',
        toolset: 'comms',
        internet: 'no',
        files: 'optional: your own observance calendar; an Excel calendar is saved if you ask',
        does: 'Plans when to post on each platform over the weeks of a campaign, ties posts to related health observances (such as National Influenza Vaccination Week), and checks draft posts for length, alt text on images, readable hashtags, emoji, words in capitals, reading level, jargon, and safe messaging when the topic is suicide.',
        when: [
          'Planning a flu, naloxone, lead, or heat-safety campaign.',
          'Checking posts before you schedule them.',
        ],
        provide: [
          'The topic, platforms, start date, number of weeks and posts per week, and any draft posts with their alt text.',
        ],
        examples: [
          'Plan a four-week flu shot campaign on Facebook and Instagram starting December 1, and write posts for the first week.',
          'Check these three posts before I schedule them: [paste posts].',
        ],
        returns: [
          'The observances in the campaign window, the posting calendar, problems in each draft post, and an Excel calendar if requested.',
        ],
        notes: [
          'Platform character limits and observance dates could not be verified by the server; confirm them. You can give your own limits and observance calendar.',
          'The assistant writes the posts; the tool plans and checks them.',
        ],
      },
      {
        id: 'draft_press_materials',
        title: 'Draft a press release, media advisory, talking points, or Q&A',
        toolset: 'comms',
        internet: 'no',
        files: 'only if you ask for a Word document',
        does: 'Builds a press release, media advisory, talking points, or spokesperson Q&A from the facts you give, marks what is missing, and checks it against CDC’s Crisis and Emergency Risk Communication (CERC) guidance: contact information, date, release time or embargo, who-what-when-where-why, one page, short sentences, no loaded words, and no client details. Adds the reading level, CERC principles, questions to prepare for, and safe messaging when the topic is suicide.',
        when: [
          'Announcing a clinic, an outbreak update, or a health advisory.',
          'Preparing a spokesperson for interviews.',
          'Reviewing a release before it goes through approval.',
        ],
        provide: [
          'The kind of material, the facts (who, what, when, where, why), key messages, quotes from officials, and the media contact; or a draft to check.',
        ],
        examples: [
          'Draft a press release announcing free flu clinics at Town Hall on Saturday.',
          'Make a media advisory for Tuesday’s press conference on naloxone training.',
          'Prepare a spokesperson Q&A for a measles case in town.',
        ],
        returns: [
          'The draft with bracketed gaps, the CERC checks, the reading grade, questions to prepare for, and the Word file name if requested.',
        ],
        notes: [
          'All facts, numbers, and quotes must come from you and go through your agency’s approval process.',
          'Never include client names or details that could identify a person.',
        ],
      },
    ],
  },
  // End of Phase 11 chapter.
  // Phase 12 chapter goes below this line.
  {
    title: 'Emergency preparedness and environmental health',
    intro: 'These thirteen tools support preparedness before, during, and after emergencies: hazard alerts, dispensing sites, hazard vulnerability assessments, shelter intake, people who rely on powered medical equipment, CASPER rapid needs assessments, ICS forms and Incident Action Plans, continuity of operations plans, HSEEP exercises and after-action reports, crisis and emergency risk communication, air quality, and heat. Eleven are in the epr group and two (air quality and heat) in the env group; the preparedness profile includes both. Staff names and positions may be used in a request, but staff phone numbers and emails belong in a roster file in your data folder: the tools put them only into the documents they write to your output folder. The guidance is drafted from FEMA, CDC, EPA, NWS, MEMA, and ResilientMass sources and is pending review by the project’s subject matter expert.',
    tools: [
      {
        id: 'get_active_hazard_alerts',
        title: 'Check active hazard alerts and what they mean for health',
        toolset: 'epr',
        internet: 'yes',
        files: 'no',
        does: 'Lists the current National Weather Service watches, warnings, and advisories for your town, county, or the whole state, and for each hazard gives the likely health effects, who is most at risk, suggested public health actions, and public messages, including the power outage and carbon monoxide risks that follow storms. It also lists FEMA disaster declarations that covered the area, with counts by type since 1953.',
        when: [
          'A storm, heat wave, flood, or other hazard is forecast or under way.',
          'Briefing your board of health, partners, or staff.',
          'Planning ahead for a season (ask for guidance for a hazard with no alert yet).',
        ],
        provide: [
          'An area (your saved town by default), and optionally hazards to plan for, such as “winter storm” or “extreme heat”.',
        ],
        examples: [
          'Are there any weather alerts for Fall River right now, and what should the health department do?',
          'What are the health effects and public messages for a hurricane warning in Barnstable County?',
          'How many FEMA disaster declarations has Bristol County had, and for what?',
        ],
        returns: [
          'Each active alert with severity, urgency, certainty, counties, timing, and NWS instructions; guidance for each hazard with citations; FEMA declarations and counts by incident type.',
        ],
        notes: [
          'NWS alerts are the official source; if the tool cannot reach them it says so plainly and shows no alerts. Never rely on an old copy for warnings.',
          'FEMA declarations count only federally declared events.',
          'If the live alert feed cannot be reached, a saved copy is used only if it is less than an hour old; otherwise the answer says alerts could not be checked. Alerts that have ended are left out.',
        ],
      },
      {
        id: 'model_dispensing_site_throughput',
        title: 'Model a dispensing site (POD) or mass vaccination site',
        toolset: 'epr',
        internet: 'no',
        files: 'only if you ask for a worksheet',
        does: 'Treats each station of a point of dispensing (greeting, forms and screening, medical consultation, dispensing, exit) or mass vaccination site (check-in, screening, vaccination) as a waiting line with several staff, and works out how many staff each station needs, how busy they are, how long people wait, where the bottleneck is, how many people the site can serve per hour, how many people are inside at once, and how many hours and days it takes to reach everyone.',
        when: [
          'Writing or updating a medical countermeasure dispensing plan.',
          'Testing a staffing plan before an exercise, or after a drill using your own station times.',
        ],
        provide: [
          'The population to reach and either the hours your sites will be open or the expected arrivals per hour, and the number of sites.',
          'Optionally: your measured minutes per client at each station, your staffing, head-of-household pickup, shift length, and support staff.',
        ],
        examples: [
          'We need to give antibiotics to 48,000 people with 2 PODs open 48 hours. How many staff per station?',
          'Our flu clinic will have 8 check-in staff, 10 screeners, and 9 vaccinators with 120 arrivals an hour. Where is the bottleneck?',
        ],
        returns: [
          'A table by station (staff, how busy, average wait, people in line, capacity), the bottleneck, site capacity, hours and days to reach everyone, staff per shift, a busiest-hour check, and CDC’s activation checklist.',
        ],
        notes: [
          'Station times are tool defaults with no published source. Replace them with times measured in your drills.',
          'The model assumes steady, random arrivals; real arrivals come in waves, so lines can be longer.',
        ],
      },
      {
        id: 'get_electricity_dependent_population',
        title: 'Find people who rely on electricity-dependent medical equipment',
        toolset: 'epr',
        internet: 'optional',
        files: 'no',
        does: 'Reports HHS emPOWER counts of Medicare beneficiaries who use electricity-dependent medical equipment (ventilators, oxygen concentrators, BiPAP, feeding and infusion pumps, home dialysis, power wheelchairs, electric beds, cardiac assist devices) for your town, county, the state, or ZIP codes, with rates compared with Massachusetts and the U.S. and power outage planning actions.',
        when: [
          'Before a storm or heat wave that may cause power outages.',
          'Planning shelters, charging sites, and outreach with emergency management and utilities.',
        ],
        provide: [
          'An area or ZIP codes. Optionally, a ZIP-level count file from DPH (for example the emPOWER Emergency Planning Dataset).',
        ],
        examples: [
          'How many people in Plymouth County rely on electricity-dependent medical equipment, by ZIP code?',
          'What are the emPOWER counts for Amherst?',
        ],
        returns: [
          'Totals, rates per 1,000 Medicare beneficiaries, a ZIP code table, planning actions, and utility phone numbers.',
        ],
        notes: [
          'Counts cover Medicare only and do not identify anyone. HHS shows 1 to 11 as 11.',
          'Files with names or addresses are refused; never put the restricted emPOWER outreach list into this tool.',
        ],
      },
      {
        id: 'hazard_vulnerability_assessment',
        title: 'Run a hazard vulnerability assessment (HVA)',
        toolset: 'epr',
        internet: 'optional',
        files: 'yes, a workbook in your output folder',
        does: 'Helps a planning group score hazards for probability, impact on people, property, and health department operations, and preparedness gaps, and ranks them by relative risk. It starts the discussion with the statewide ratings in the 2023 ResilientMass Plan, FEMA declarations for your county, and, if you download one, FEMA National Risk Index scores.',
        when: [
          'Your annual or HMCC hazard vulnerability assessment.',
          'Updating a preparedness or continuity plan.',
        ],
        provide: [
          'A town or county and, as the group decides them, scores for each hazard. Optionally a National Risk Index table saved in your data folder.',
        ],
        examples: [
          'Start a hazard vulnerability assessment for Worcester County.',
          'Score winter storms 3, 2, 2, 2 with preparedness 1, 2, 2, and pandemic 2, 3, 1, 3 with 2, 2, 2. Rank them.',
        ],
        returns: [
          'A ranked table, seeds for discussion for 21 hazards, the scoring scales, and an Excel facilitation workbook.',
        ],
        notes: [
          'The scoring method is the tool’s own, modeled on common HVA worksheets. Use your HMCC’s method if it differs.',
          'The National Risk Index could not be bundled because FEMA’s download site was unavailable.',
        ],
      },
      {
        id: 'functional_needs_shelter_intake',
        title: 'Shelter health intake for access and functional needs (CMIST)',
        toolset: 'epr',
        internet: 'optional',
        files: 'yes, to your output folder',
        does: 'Provides a shelter health intake organized by the CMIST framework (communication, maintaining health, independence, support and safety, transportation) as a blank form, fills completed forms from your registration file without showing anyone’s details in the chat, and estimates how many residents of a shelter of a given size will need interpreters, accessible cots, personal assistance, power for medical equipment, and other support.',
        when: [
          'Preparing shelter health services or a shelter plan.',
          'During sheltering, to turn registration data into forms and a count of needs.',
        ],
        provide: [
          'The town or county the shelter serves and the expected number of residents.',
          'Optionally, a registration file in your data folder with one row per person and columns named like the questions.',
        ],
        examples: [
          'We expect 200 people at the Brockton shelter. What functional needs should we plan for, and how many staff?',
          'Make the CMIST intake form for our shelter.',
          'Fill intake forms from shelter_registrations.csv.',
        ],
        returns: [
          'The intake questions, estimates by need with local rates, staff per shift, and, from a file, counts of needs and preferred languages (small numbers hidden).',
        ],
        notes: [
          'Completed forms go only to your output folder. Agency translations in templates/<language>/cmist_intake.md are used when present; otherwise forms are in English and the tool says how many.',
          'The questions are a draft and the staffing ratios are tool defaults; adapt them to your shelter plan.',
          'The translation folder is the language as written in your registration file, in lower case with an underscore between words (Haitian Creole → templates/haitian_creole/). A file that uses codes such as “es” needs a folder named es.',
        ],
      },
      {
        id: 'plan_casper_assessment',
        title: 'Plan and analyze a CASPER rapid needs assessment',
        toolset: 'epr',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Plans a CDC Community Assessment for Public Health Emergency Response (CASPER): selects 30 census blocks with probability proportional to housing, gives each cluster’s households and sampling interval, and writes the cluster list, tracking forms, and a team packet. After fieldwork it weights the returned data as the CASPER Toolkit describes and reports estimated households, percentages, and 95% confidence intervals.',
        when: [
          'After a disaster, to measure household needs quickly.',
          'For a preparedness survey of your community.',
        ],
        provide: [
          'A town, several towns, or a county; optionally the number of clusters, total or occupied households, and a seed to reproduce a selection.',
          'For analysis: a file of completed questionnaires with a cluster column, the questions to estimate, and tracking-form totals.',
        ],
        examples: [
          'Plan a CASPER for Fall River.',
          'Analyze casper_results.csv for the questions damage and generator, with 300 households attempted and 200 contacted.',
        ],
        returns: [
          'The selected clusters, the seed, household selection steps, weights, response-rate formulas, and, from returned data, weighted results with confidence intervals and response rates.',
        ],
        notes: [
          'Uses 2020 Census housing counts; homes built or lost since then are not reflected.',
          'Results with fewer than 5 answers are not shown, as the toolkit advises.',
          'Answers with fewer responses than the larger of 5 and your suppression threshold are hidden, and when only one answer is that small, the next smallest is hidden too.',
          'Analyzing returned data without your plan’s seed does not draw a new sample or write new materials.',
        ],
      },
      {
        id: 'generate_ics_forms',
        title: 'Fill ICS forms and assemble an Incident Action Plan',
        toolset: 'epr',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Fills FEMA Incident Command System forms for a health department incident (ICS 201 Incident Briefing, 202 Incident Objectives, 203 Organization Assignment List, 204 Assignment List, 205 Radio Communications Plan, 205A Communications List, 206 Medical Plan, 207 Organization Chart, 208 Safety Message, 213 General Message, 213RR Resource Request, 214 Activity Log, and 215A Safety Analysis) and assembles the Incident Action Plan as a Word or PDF document. The forms keep FEMA’s block numbers and labels. It tells you which required blocks are still blank and suggests SMART objectives and safety precautions for common public health incidents.',
        when: [
          'At the start of a response, to brief staff and set objectives for the first operational period.',
          'Before each planning meeting, to produce the next Incident Action Plan.',
          'During an exercise, so staff practice with the same forms they will use in a real response.',
        ],
        provide: [
          'The incident name, the operational period (start and end date and time), and your objectives.',
          'Who holds each ICS position (positions and names only), the Divisions or Groups and their work, radio channels, and the responder medical plan (hospitals and ambulance service).',
          'Optionally: a staff roster file in your data folder with phone numbers and emails, the kind of incident (for example measles, POD, hurricane, heat), a map image, and whether you want blank forms too.',
        ],
        examples: [
          'Make an Incident Action Plan for the Maple Grove measles exposure for tomorrow 0700 to 1900, using staff_roster.csv for contacts.',
          'Write an ICS 213RR for 200 cases of bottled water for the senior center cooling site, urgent.',
          'Give me blank ICS 214 activity logs and an ICS 215A with suggested hazards for a coastal storm.',
        ],
        returns: [
          'One file per form and the assembled Incident Action Plan in your output folder, a list of required blocks still blank with FEMA’s instructions, an Incident Action Plan checklist, and suggested objectives and safety lines with their sources.',
        ],
        notes: [
          'Phone numbers and emails come only from your roster file and go only into the documents; they never appear in the chat. Do not type them into your request.',
          'The forms are accessible versions of FEMA’s forms, not FEMA’s fillable PDFs. Signatures are added by hand.',
          'Suggested objectives and hazards are drafts for the Incident Commander and Safety Officer to review.',
          'A person’s contact details are filled in only when the roster has the exact full name you gave. A name not on the roster gets no contact details, never those of someone else in the same position. Check the spelling if contacts are missing.',
        ],
      },
      {
        id: 'continuity_of_operations_planner',
        title: 'Draft a continuity of operations (COOP) plan',
        toolset: 'epr',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Drafts a continuity of operations plan for your board of health or health department, organized by FEMA’s ten continuity elements. It starts from 19 board of health essential functions with their legal authority (for example 24/7 disease reporting under 105 CMR 300.000, isolation and quarantine, burial permits, and housing and food emergencies), puts them in priority tiers with recovery times, drafts orders of succession and delegations of authority (including the board’s emergency agent under M.G.L. c. 111, section 30), checks whether your staff can cover the most urgent functions if 40% are out in a pandemic, and builds a dated schedule of tests, training, and exercises. It tells you which parts of the plan are done, drafted, or still missing.',
        when: [
          'Writing or updating your COOP plan, or checking it against FEMA’s elements.',
          'Before flu season or a predicted storm, to confirm who takes over and where staff will work.',
          'Planning for staff shortages in an outbreak or pandemic.',
        ],
        provide: [
          'Optionally: your town and agency name, changes to the essential functions (tier, recovery time, staff), your key positions and their successors, alternate sites and telework, the number of staff, and a devolution partner.',
          'Optionally: a staff roster file (CSV or Excel) with names, positions, phones, and emails. Contact details go only into the Word plan and Excel workbook in your output folder.',
        ],
        examples: [
          'Draft a COOP plan for the Wilmington Board of Health. We have 6 staff, and the library meeting room is our alternate site.',
          'Make our health director succession three deep: health agent, public health nurse, then the board chair. Use staff_roster.csv for contacts.',
          'Move routine immunization clinics to Tier 2 and add animal bite follow-up as Tier 1.',
        ],
        returns: [
          'Essential functions by tier with recovery times, authority, and staff; orders of succession; delegations of authority; sites, communications, and essential records; the staffing check; the first year’s test, training, and exercise dates; a pandemic annex; and a list of plan elements that are provided, drafted, or missing.',
          'A Word plan (with a call-down annex when you give a roster) and an Excel workbook in your output folder.',
        ],
        notes: [
          'A draft for the board to review and adopt. Tiers, recovery times, and staff numbers are starting points; set your own from a business impact analysis.',
          'Delegations of authority have legal effect. Have town counsel confirm each one before the plan is adopted.',
          'Give staff phone numbers and emails in the roster file, not in the chat.',
          'FEMA’s continuity guidance could not be downloaded when this content was written; the statutes and CDC sources were checked. Pending review.',
          'Contact details for a successor named in your request are filled in only when the roster has that exact full name.',
        ],
      },
      {
        id: 'design_hseep_exercise',
        title: 'Design a tabletop or other preparedness exercise (HSEEP)',
        toolset: 'epr',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Designs an exercise that follows FEMA’s Homeland Security Exercise and Evaluation Program (HSEEP): checks that your objectives are SMART, links each one to the CDC public health preparedness (PHEP) capabilities and FEMA core capabilities, and builds the scenario, modules, discussion questions, schedule, and planning timeline. It writes the Situation Manual and Facilitator Guide (or the Exercise Plan and Master Scenario Events List for drills and functional exercises), Exercise Evaluation Guides, a participant feedback form, and a sign-in sheet. Three tabletop scenarios are ready to use: a measles outbreak, a coastal storm with a power outage, and extreme heat.',
        when: [
          'Planning a tabletop, drill, or functional exercise for your health department, school partners, or HMCC.',
          'Turning an idea for an exercise into HSEEP documents with objectives that will hold up in the after-action report.',
        ],
        provide: [
          'The scenario (measles, coastal storm, extreme heat, or another hazard) or your own scenario in a few sentences; your town; the date, place, and length.',
          'Optionally: your own objectives, the people and organizations taking part (positions are enough), and a staff roster file in your data folder for contact details.',
        ],
        examples: [
          'Design a measles tabletop exercise for Amherst on March 17, 2027, at Town Hall, for 3.5 hours.',
          'Plan a functional exercise on extreme heat for Lowell, with an MSEL.',
          'Check whether these exercise objectives are SMART and which capabilities they test.',
        ],
        returns: [
          'The objectives with their capabilities and a SMART check, the scenario modules with injects and questions, the day’s schedule, planning meeting dates, and the list of documents written.',
        ],
        notes: [
          'Scenario events are fictional and written by this project; have your planning team and DPH regional staff review them before use.',
          'Links between PHEP and FEMA capabilities are this project’s mapping, not an official crosswalk. Planning meeting weeks are suggestions.',
          'Staff phone numbers and emails from your roster appear only in the documents, never in the chat.',
          'If the schedule needs more time than the length you gave, the answer says how many hours it needs.',
        ],
      },
      {
        id: 'build_after_action_report',
        title: 'Write the after-action report and improvement plan',
        toolset: 'epr',
        internet: 'no',
        files: 'yes, to your output folder',
        does: 'Turns the strengths and areas for improvement from your hotwash and evaluators into an HSEEP after-action report and improvement plan for an exercise or a real incident. It checks that every area for improvement has a corrective action with an owner and dates, that findings describe the problem rather than the fix, and that no names or client details slipped in. It totals participant feedback forms from your file and writes the report (Word or PDF) and an improvement plan tracker (Excel).',
        when: [
          'After an exercise hotwash, to draft the AAR/IP for the after-action meeting.',
          'After a real response, such as an outbreak or a storm, to capture lessons learned.',
        ],
        provide: [
          'The objectives with their capabilities and ratings (P, S, M, or U), the observations, and the corrective actions with the responsible organization, point of contact, and dates. Share notes without client names or details.',
          'Optionally, a file of participant feedback forms and a staff roster in your data folder.',
        ],
        examples: [
          'Write the after-action report for our measles tabletop from these hotwash notes.',
          'Add the feedback forms in exercise_feedback.csv to the after-action report.',
          'Check whether our improvement plan has an owner and a due date for every area for improvement.',
        ],
        returns: [
          'The capability ratings, the numbered improvement plan with what is missing for each action, the HSEEP checks, feedback form averages (small numbers hidden), and next steps with suggested dates.',
        ],
        notes: [
          'The report is a draft for the after-action meeting; leaders confirm findings, owners, and dates.',
          'Written comments on feedback forms are counted but not shown in the chat; read them in your file.',
          'The 30-, 45-, and 60-day after-action dates are suggestions; follow your grant or program deadlines.',
          'You can pass the objectives from design_hseep_exercise unchanged. A rating for a capability the tool does not recognize is an error, so it is not dropped silently.',
        ],
      },
      {
        id: 'crisis_risk_communication_builder',
        title: 'Build crisis and emergency risk communication (CERC)',
        toolset: 'epr',
        internet: 'optional',
        files: 'yes, to your output folder',
        does: 'Uses CDC’s Crisis and Emergency Risk Communication (CERC) model to build a message map (three key messages with three supporting facts each), a holding statement, frequently asked questions, spokesperson preparation, and the communication tasks for each phase of a crisis. It has ready-made message sets for a measles exposure, a boil water order, extreme heat, a coastal storm with power outages and carbon monoxide, shelter in place, respiratory illness, and mass dispensing, and it checks every message against message-mapping rules: 27 words in about 9 seconds, empathy first, short sentences, your reading grade, no absolutes, and positive action steps. It lists your town’s priority languages and uses your agency’s approved translations when you have them; otherwise it writes a brief for a qualified translator.',
        when: [
          'In the first hours of an emergency, to get an accurate first statement out quickly.',
          'Before an emergency, to prepare and test messages and have them translated.',
          'To check a message someone has drafted before it is released.',
        ],
        provide: [
          'The hazard (or a short description of another one) and the phase: preparation, initial, maintenance, or resolution.',
          'What you know, what you do not know yet, what you are doing, when the next update will come, and where people can get information.',
          'Details for the message, such as the place and times of a measles exposure. Never client names.',
          'Optionally: your staff roster file, so the media contact’s phone and email go into the Word packet only.',
        ],
        examples: [
          'We had a measles exposure at the Pollard Library on Tuesday from 2 to 5 p.m. Draft the first message for Lowell.',
          'Check this boil water notice against CERC before we send it.',
          'Prepare maintenance-phase messages for a coastal storm with power outages, in Word, with translator briefs.',
        ],
        returns: [
          'A message map with its checks, a holding statement with any missing parts marked in brackets, FAQs, spokesperson tips and likely reporters’ questions, audience concerns, the CERC tasks for the phase, and a language plan; optionally a Word packet, the English template, and translations or translator briefs in your output folder.',
        ],
        notes: [
          'Every message is a draft for your agency’s approval; check each fact with your subject matter experts.',
          'Nothing is machine-translated. Save approved translations as templates/<language>/cerc_<message set>.md in your data folder to reuse them.',
          'The mass dispensing messages depend on your POD plan; replace the low-confidence parts with your plan’s wording.',
          'Your agency’s approved translation is used only for the library’s own English message. If you change the key messages, action steps, or empathy statement, a translator brief is prepared instead, because the approved translation no longer matches.',
          'A description such as “space heater fire” does not bring up the extreme heat messages; name the message set if one fits.',
        ],
      },
      {
        id: 'air_quality_protective_guidance',
        title: 'Check air quality and protective actions',
        toolset: 'env',
        internet: 'optional',
        files: 'no',
        does: 'Shows the current and forecast Air Quality Index from EPA AirNow for the reporting area nearest your town (or every Massachusetts area), and the protective actions for sensitive groups, the public, schools, and outdoor events for that level. It can also convert a pollutant reading to an AQI.',
        when: [
          'Wildfire smoke or an ozone day.',
          'Advising schools, camps, coaches, or event organizers.',
        ],
        provide: [
          'An area, or an AQI value or pollutant reading you already have.',
        ],
        examples: [
          'What is the air quality in Springfield today, and should schools hold recess outside?',
          'The AQI is 135 for ozone. What should we tell outdoor event organizers?',
          'Convert a 24-hour PM2.5 of 60 µg/m³ to an AQI.',
        ],
        returns: [
          'Current, yesterday’s, and forecast AQI by pollutant, the category, sensitive groups, actions by audience, and tips.',
        ],
        notes: [
          'AirNow data are preliminary. Without the internet, actions for every category are listed.',
          'Some actions for the public and events are the project’s summaries of EPA and CDC guidance.',
          'Very high 8-hour ozone or 1-hour sulfur dioxide readings are refused with the averaging time to use instead, because EPA’s tables do not cover them.',
        ],
      },
      {
        id: 'heat_health_action_planner',
        title: 'Plan the heat response',
        toolset: 'env',
        internet: 'optional',
        files: 'yes, a checklist in your output folder',
        does: 'Combines the National Weather Service 7-day HeatRisk and forecast for your town or county with your local heat-vulnerable population (older adults, young children, disability, poverty, no car, limited English, chronic disease, socially vulnerable areas, and people who rely on powered medical equipment) and gives public health steps for the expected heat level. It writes a Word checklist with cooling centers, outreach call and door-knock scripts, school and outdoor worker guidance, heat illness teaching, and medication and power outage planning.',
        when: [
          'Before a heat wave, or at the start of summer to prepare.',
          'Exercising your heat plan (choose a HeatRisk level).',
        ],
        provide: [
          'A town or county, and optionally your cooling center list (a CSV or Excel file in your data folder).',
        ],
        examples: [
          'Make a heat response plan for Worcester for this week.',
          'Plan for HeatRisk red (3) in Lowell using cooling_centers.xlsx.',
        ],
        returns: [
          'HeatRisk by day, the forecast, heat alerts, the local population table, activation steps, and the checklist file name.',
        ],
        notes: [
          'HeatRisk is experimental and does not replace NWS heat advisories and warnings.',
          'Activation steps and scripts are drafts; adapt them to your heat plan. Cooling center rows go into the checklist only.',
          'If the alert feed cannot be reached, the checklist says “NWS alerts could not be checked — see weather.gov”, not “none”.',
        ],
      },
    ],
    extras: [
      {
        title: 'Guided workflow: plan a tabletop exercise',
        blocks: [
          {
            p: 'The server also offers a ready-made **prompt** called “Plan a tabletop exercise.” It walks Claude through a full exercise cycle from FEMA’s Homeland Security Exercise and Evaluation Program (HSEEP), adapted to a Massachusetts local health department working with DPH, the HMCC, and MEMA, and has Claude use the tools in this chapter in order.',
          },
          {
            p: 'In Claude Desktop, choose “Plan a tabletop exercise” from the same menu as the outbreak investigation prompt (chapter 6) and fill in whatever you know; every item is optional:',
          },
          {
            bullets: [
              'the scenario (for example a measles outbreak, a coastal storm with a power outage, or extreme heat);',
              'the town or county;',
              'the exercise date;',
              'a staff roster file in your data folder, so contact details go only into the documents.',
            ],
          },
          {
            p: 'The workflow covers choosing the hazard and describing the people most affected; designing the exercise with objectives linked to CDC and FEMA capabilities; preparing player materials (an Incident Action Plan, crisis messages, and, when an inject calls for it, the continuity plan, a clinic or POD model, or shelter intake); the hotwash; and the after-action report and improvement plan.',
          },
        ],
      },
    ],
  },
  // End of Phase 12 and 13 chapter.
  // Phase 14 chapter goes below this line.
  {
    title: 'Environmental health and climate',
    intro: 'These eight tools cover environmental health and the health effects of climate: drinking water quality and community water fluoridation, environmental justice populations, climate and health vulnerability for MVP and hazard mitigation planning, mosquito- and tick-borne disease risk, housing conditions under the State Sanitary Code, chemical exposure reference values, and food access. Six are in the env group (the preparedness profile includes it; turn it on for other profiles); the fluoridation and food access tools are in the chronic group (phn_leader only). Every figure names its source and date, and the tools work offline from bundled snapshots when live data cannot be reached. The guidance was drafted from EPA, MassDEP, DPH, EEA, CDC, ATSDR, NIOSH, FEMA, and USDA sources retrieved in November 2026 and is pending review by the project’s subject matter expert; every answer says so.',
    tools: [
      // Phase 14 Water (ENV-03, CHR-07) tools.
      {
        id: 'drinking_water_quality_lookup',
        title: 'Look up drinking water quality',
        toolset: 'env',
        internet: 'optional',
        files: 'no',
        does: 'Lists the public water systems in a Massachusetts town (from EPA’s drinking water database) with how many people each serves, health-based violations in recent years, the latest lead and copper test results compared with the action levels, and MassDEP PFAS results for treated water compared with the Massachusetts standard of 20 ng/L. It says what residents can do and, when part of the town uses private wells, what MassDEP recommends well owners test for and how often.',
        when: [
          'A resident, parent, or provider asks whether the town’s water is safe, or about PFAS or lead in the water.',
          'Planning outreach to families with infants or pregnant people where PFAS is above the standard.',
          'Advising private well owners, home buyers, or landlords about well testing.',
        ],
        provide: [
          'A town (or use your saved town), or a water system’s ID (PWSID) or name.',
          'Optionally how many years of violations to show, or ask to include schools, child care, and businesses with their own wells.',
        ],
        examples: [
          'Is there PFAS in Boxborough’s water, and what should pregnant residents do?',
          'Show health-based drinking water violations in Acton over the last 10 years.',
          'What should private well owners in Harvard test for, and how often?',
          'Look up water system MA2002000.',
        ],
        returns: [
          'A table of systems with violations, lead results, and PFAS results; what needs attention; what residents can do; private well testing guidance when relevant; and the current Massachusetts and EPA standards, including EPA’s 2026 proposed changes to the PFAS rule.',
        ],
        notes: [
          'EPA’s data are updated every three months and can lag MassDEP; the water system and MassDEP have the final word on compliance.',
          'Without the internet, a dated copy of the data is used and the answer says so.',
          'PFAS averages are for information only; MassDEP decides compliance from quarterly averages of monthly samples.',
          'Systems are listed under the town in their ID number; a neighboring town’s system may also serve part of your town.',
        ],
      },
      {
        id: 'community_water_fluoridation_status',
        title: 'Check community water fluoridation',
        toolset: 'chronic',
        internet: 'no',
        files: 'only if you ask for a brief',
        does: 'Shows whether a town’s water is fluoridated on DPH’s list and in CDC’s state report (which systems add fluoride, buy fluoridated water, or have natural fluoride, and their reported fluoride levels), the statewide picture, each organization’s current position side by side with its date, the main evidence reviews, where they disagree, and what Massachusetts law (M.G.L. c. 111, § 8C) says about local decisions. It can write a Word brief for a board of health discussion.',
        when: [
          'A board of health is asked to start, continue, or stop fluoridation.',
          'Deciding with local clinicians whether children need other fluoride protection.',
          'Answering residents’ questions about fluoride in the water.',
        ],
        provide: [
          'A town (or use your saved town), or “Massachusetts” for the statewide summary.',
          'Ask for a brief if you want the Word document.',
        ],
        examples: [
          'Is Hingham’s water fluoridated?',
          'Prepare a fluoridation brief for our board of health in Northampton.',
          'What share of Massachusetts residents get fluoridated water?',
        ],
        returns: [
          'The town’s status from DPH and CDC with the systems behind it, the statewide summary, positions from DPH, CDC, EPA, FDA, the AAP, and the Community Preventive Services Task Force with dates, evidence reviews (NTP, JAMA Pediatrics, Cochrane, CDC), where they differ, the state law, and draft discussion points.',
        ],
        notes: [
          'The tool never takes a side; it shows each body’s position and date so the board can weigh them.',
          'Federal positions changed in 2025 and 2026; check the sources before a vote.',
          'DPH’s list is from 2023 and CDC’s report is reported by the state; the water system is the best source for current fluoride levels.',
          'Some organizations’ websites could not be reached, so their positions are not included (the answer lists them).',
        ],
      },
      // End of Phase 14 water tools.
      // Phase 14 EJ and climate (ENV-04, ENV-05) tools.
      {
        id: 'environmental_justice_screen',
        title: 'Screen for environmental justice populations',
        toolset: 'env',
        internet: 'no',
        files: 'optional: a map and a table in your output folder; reads an EJScreen file you saved',
        does: 'Shows which neighborhoods (census block groups) in your town, a county, or the state are Massachusetts environmental justice (EJ) populations, which criteria they meet (minority, income, English isolation), how many people live in them, and the languages to plan for. It explains what an EJ designation changes (state environmental review of nearby projects, translation and interpreters, DPH’s vulnerable health criteria) and adds Social Vulnerability Index context.',
        when: [
          'Planning outreach, a community health assessment, or language access.',
          'A project needing state environmental review (MEPA) is proposed near your town and you want to know whether EJ rules apply.',
          'Writing a grant that asks about EJ populations.',
        ],
        provide: [
          'A town, county, or "Massachusetts" (your saved town is the default).',
          'Optionally, an archived EPA EJScreen block group file you saved in your data folder, and whether you want a map or a table.',
        ],
        examples: [
          'Which neighborhoods in Lowell are environmental justice populations, and which languages should we plan for?',
          'How many people in Hampden County live in EJ block groups? Make a map.',
          'Using the EJScreen file ejscreen_ma.csv, compare traffic and air pollution percentiles in Chelsea’s EJ block groups with the rest of the city.',
        ],
        returns: [
          'EJ block groups and people by criterion, the share of residents, limited-English households and top languages, towns ranked by EJ population (for a county or the state), SVI context, what designation means, and DPH’s vulnerable health criteria.',
          'With an EJScreen file: average percentiles for EJ and other block groups and counts at or above the 80th percentile (no rows).',
        ],
        notes: [
          'The EJ layer uses the 2020 Census and 2016–2020 survey estimates (EEA update of November 2022). Confirm a specific site on EEA’s EJ Maps Viewer.',
          'EPA removed EJScreen in 2025. It is not built in; if you have an archived copy, save just the Massachusetts rows in your data folder.',
          'The CDC Environmental Justice Index is not built in; the link is given.',
        ],
      },
      {
        id: 'climate_health_vulnerability_profile',
        title: 'Profile your town’s climate and health vulnerability',
        toolset: 'env',
        internet: 'optional',
        files: 'optional: a Word profile in your output folder',
        does: 'Builds a one-page-style profile of your town for Municipal Vulnerability Preparedness (MVP) and hazard mitigation planning: extreme heat days, tree canopy, and paved surface; flood insurance claims and flood program status; environmental justice and social vulnerability; older adults, young children, disability, poverty, no car, limited English; asthma, COPD, heart disease, diabetes and other climate-sensitive conditions; people who rely on powered medical equipment; and your town’s MVP and FEMA hazard mitigation plan status. Every number is compared with Massachusetts, and the most important vulnerabilities are listed first.',
        when: [
          'Your town is starting or updating its MVP or hazard mitigation plan.',
          'Preparing a climate and health section for a community health assessment or a grant.',
        ],
        provide: [
          'A city or town (your saved town is the default). Ask for a Word document if you want one.',
        ],
        examples: [
          'Build a climate and health vulnerability profile for Chelsea for our MVP workshop.',
          'Is Scituate’s hazard mitigation plan current, and how often has it flooded?',
          'Write a climate vulnerability profile for Pittsfield as a Word document.',
        ],
        returns: [
          'Heat, flooding, social vulnerability, and health indicators with Massachusetts comparisons, sources, and years; the most important vulnerabilities and the rule that flagged each; MVP and FEMA plan status; health effects of heat and flooding.',
        ],
        notes: [
          'The rules that pick the most important vulnerabilities are the tool’s defaults (for example, at least 1.2 times the state value), not a standard; your planning team can weigh them differently.',
          'Heat and tree canopy values come from ZIP code areas averaged over the town. Flood insurance claims show past insured losses, not a flood map.',
          'FEMA plan status is checked online when possible; otherwise a saved copy is used and the answer says so.',
        ],
      },
      // End of Phase 14 ejclimate tools.
      // Phase 14 Vectors and housing (ENV-06, ENV-07) tools.
      {
        id: 'vector_borne_disease_risk',
        title: 'Check mosquito- and tick-borne disease risk',
        toolset: 'env',
        internet: 'optional',
        files: 'optional, a one-page notice in your output folder',
        does: 'Shows the Massachusetts Department of Public Health (DPH) risk level for Eastern equine encephalitis (EEE) and West Nile virus in your town, county, or the whole state, what the level means, what residents can do, and what DPH’s Arbovirus Surveillance and Response Plan recommends at that level. It adds Lyme disease, babesiosis, and anaplasmosis cases and rates for your county from DPH’s yearly reports, emergency department visits for tick-borne disease, and prevention messages for families, schools, camps, and outdoor workers.',
        when: [
          'During mosquito season (June to October), when DPH raises a town’s risk level.',
          'Planning tick and mosquito messages for schools, camps, or outdoor workers.',
          'Preparing a board of health update or a community health assessment.',
        ],
        provide: [
          'A town or county (or your saved town), and who the messages are for.',
          'Optionally a past season (2022 or later), or ask for a one-page notice.',
        ],
        examples: [
          'What is the EEE and West Nile virus risk in Middleborough right now?',
          'Which towns in Plymouth County were at high EEE risk in 2024?',
          'How common is babesiosis in Barnstable County, and what should camps do about ticks?',
          'Make a one-page mosquito and tick notice for our schools.',
        ],
        returns: [
          'Risk levels with the date DPH last updated them, what each level means and the actions for it, positive mosquito and animal findings, human cases by county, earlier seasons, tick-borne disease counts and rates by county with the state rate, emergency department visits, and cited prevention messages.',
        ],
        notes: [
          'Outside June to October, or when DPH’s page cannot be reached, it shows DPH’s last levels with their date and says clearly that they are not current risk.',
          'DPH publishes tick-borne disease cases by county, not town, and reports Dukes and Nantucket together. Counts under 20 are marked unstable.',
          'Lyme disease counts changed in 2022 when the case definition changed, so they cannot be compared with earlier years.',
          'The recommended cancellation times for evening activities are DPH’s 2026 schedule; DPH publishes a new one each season.',
        ],
      },
      {
        id: 'housing_conditions_health_guide',
        title: 'Explain unhealthy housing conditions and how to get an inspection',
        toolset: 'env',
        internet: 'no',
        files: 'yes, tenant teaching sheets (and letters, if you give a household file) in your output folder',
        does: 'Explains what the State Sanitary Code (105 CMR 410.000) requires for mold and leaks, pests, heat, hot water and utilities, lead paint, smoke and carbon monoxide alarms, and trash: who must fix what (owner or tenant), the health effects and who is most at risk, which problems are urgent, how to get a free board of health inspection and how fast it must happen, and tenant protections. It writes plain-language tenant teaching sheets checked for reading level, and can fill inspection request and repair request letters from your own household list.',
        when: [
          'A family on your caseload has mold, pests, no heat, or peeling paint.',
          'Teaching a tenant how to ask for repairs or an inspection.',
          'Preparing handouts for a clinic, shelter, or community event.',
        ],
        provide: [
          'The problems in the family’s own words (for example “roaches and a leak”), or leave it blank for all.',
          'Optionally the languages you need (only your agency’s approved translations are used) and a household file in your data folder for the letters.',
        ],
        examples: [
          'A family in Holyoke has no heat and mice. What does the sanitary code require, and how do they get an inspection?',
          'Make teaching sheets about mold and lead paint in English and Spanish.',
          'Fill inspection request letters from housing_requests.csv.',
        ],
        returns: [
          'Code sections and requirements, owner and tenant duties, health effects, inspection and hearing timelines, tenant protections, local housing facts, the teaching sheets with their reading grade, and the letter templates.',
        ],
        notes: [
          'Tenant protections are general information, not legal advice. Refer families to legal aid (masslrf.org) before they withhold rent or make repairs themselves.',
          'Names, addresses, and phone numbers from your household file go only into the letters in your output folder; they are never shown in the chat.',
          'Save your board of health phone number (configure_jurisdiction) so it prints on the sheets.',
        ],
      },
      // End of Phase 14 vectorhousing tools.
      // Phase 14 Chemicals and food (ENV-08, CHR-06) tools.
      {
        id: 'chemical_exposure_reference',
        title: 'Look up a chemical exposure',
        toolset: 'env',
        internet: 'no',
        files: 'no',
        does: 'Looks up a chemical by name, synonym, or CAS number and shows its NIOSH Pocket Guide entry (workplace limits, symptoms, target organs, first aid), the current NIOSH IDLH, ATSDR minimal risk levels by route and length of exposure, and links to the ATSDR ToxFAQs fact sheet you can share. For a spill, indoor air, contaminated site, drinking water, or workplace question it adds notes and whom to call in Massachusetts.',
        when: [
          'A resident calls about a spill, an odor, or a chemical found at a site or in water.',
          'You need plain-language material (ToxFAQs) for a community meeting.',
          'Checking what a chemical does to the body and how it is absorbed.',
        ],
        provide: [
          'The chemical name, a synonym or abbreviation (TCE, perc), or the CAS number from the Safety Data Sheet.',
          'Optionally the situation: spill or release, indoor air, contaminated site, drinking water, or workplace.',
        ],
        examples: [
          'A truck spilled toluene on Route 9. What are the health effects and whom do we call?',
          'Residents near a dry cleaner are worried about perc in indoor air. What are the ATSDR screening levels?',
          'Look up CAS 79-01-6 for a contaminated site question.',
        ],
        returns: [
          'The NIOSH entry, current IDLH, ATSDR MRLs, ToxFAQs links, scenario notes, a whom-to-call table, and how to read the numbers.',
        ],
        notes: [
          'Workplace limits (REL, PEL, IDLH) are for adult workers over a work shift. They are not safe levels for homes, schools, or communities; every answer says so.',
          'ATSDR minimal risk levels are screening levels, not cleanup levels or a test for whether one person was harmed.',
          'If a name could mean more than one chemical (for example arsenic), the tool lists the choices instead of guessing.',
          'Phone numbers were checked on official pages when the tool was built; the content is waiting for review.',
        ],
      },
      {
        id: 'food_access_mapper',
        title: 'Map food access and food resources',
        toolset: 'chronic',
        internet: 'no',
        files: 'yes, a map, a Word resource list, and an Excel workbook in your output folder',
        does: 'Shows the census tracts in your town, county, or the state that the USDA Food Access Research Atlas counts as low income and low access to food (four definitions, by straight-line or driving distance), how many people live there, households without a car or receiving SNAP, and how these tracts overlap social vulnerability. It adds Healthy Incentives Program (HIP) and SNAP information, and counts the food pantries, SNAP stores, and HIP farm vendors in your own lists by town.',
        when: [
          'A community health assessment or improvement plan on food insecurity.',
          'Planning where to hold a mobile pantry, farmers market, or SNAP outreach.',
          'A grant application that needs food access data.',
        ],
        provide: [
          'A town or county (or nothing for your saved town).',
          'Optionally your lists in your data folder: food pantries and meal sites, SNAP stores downloaded from USDA, and HIP vendors. List organizations only, never clients.',
        ],
        examples: [
          'Which census tracts in Holyoke are low income and low access to food?',
          'Map food access in Hampden County by driving distance and use my pantries.csv list.',
          'How many HIP vendors and food pantries do we have in the low-access areas of Springfield?',
        ],
        returns: [
          'Tract counts and people by definition, a list of the low-access tracts, HIP and SNAP facts, counts of your sites by town, a tract map, a Word resource list, and an Excel workbook.',
        ],
        notes: [
          'The newest map (2025) counts any SNAP store, including convenience stores; the older 2019 supermarket map is shown for comparison and is approximate on today’s census tracts.',
          'Site names and addresses from your lists go only into the files in your output folder; the chat sees counts.',
          'A list that looks like clients (names with dates of birth, client or patient columns) is refused.',
        ],
      },
      // End of Phase 14 chemfood tools.
    ],
  },
  // End of Phase 14 chapter.
  // Phase 15 chapter goes below this line.
  {
    title: 'Chronic disease prevention and community assessment',
    intro: 'TODO: coordinator writes the chapter introduction at the end of the phase.',
    tools: [
      // Phase 15 screen (CHR-02, CHR-03) tools.
      {
        id: 'community_bp_screening_toolkit',
        title: 'Run a community blood pressure screening',
        toolset: 'chronic',
        internet: 'no',
        files: 'yes, a blank screening kit in your output folder (and follow-up letters, if you give a screening log)',
        does: 'Gives you a ready kit for a community blood pressure screening: a checklist for taking an accurate reading, the current ACC/AHA blood pressure categories for adults, the wording for a severe-range reading, referral tiers, a screening log template, follow-up letters, and a plain-language handout checked for reading level. It can sort readings you type in, add up a screening log you keep, and fill follow-up letters from that log. Each category shows where it came from and how sure the tool is. The referral tiers are the project’s own defaults and are marked “tool default, pending review.”',
        when: [
          'Planning a screening at a church, senior center, or health fair.',
          'Checking what category a set of readings falls in.',
          'Summing up a screening event, or sending follow-up letters to people with high readings.',
        ],
        provide: [
          'Nothing to get the blank kit. For one person, the readings (numbers only), and whether the person is pregnant or has symptoms.',
          'For a whole event, a screening log in your data folder with age, the systolic and diastolic numbers, and a referral status. A name and address column is needed only for letters.',
        ],
        examples: [
          'Make a blood pressure screening kit for our health fair.',
          'Two readings were 146/92 and 142/90. What category is that, and what should I do?',
          'Summarize bp_screening_log.csv and write the follow-up letters.',
        ],
        returns: [
          'The categories, the severe-range wording, the technique checklist, the referral tiers (marked as defaults to review), the log template columns, the letter templates, the files written, and, for a log, counts by category, age group, and referral status with small counts hidden.',
        ],
        notes: [
          'The 2025 AHA/ACC guideline replaced the 2017 one. The tool could not read its full text: normal, elevated, and the 130/80 cut-off come from CDC’s pages, and the stage 1, stage 2, and severe-range numbers come from published papers that report the guideline. Each item shows its confidence.',
          'Referral timing is for your agency’s standing orders and medical director. The tool’s tiers are drafts. This is decision support, not medical advice.',
          'Names and addresses from your log go only into the letters in your output folder; they are never shown in the chat.',
        ],
      },
      {
        id: 'diabetes_prevention_finder',
        title: 'Score the prediabetes risk test and find a diabetes prevention program',
        toolset: 'chronic',
        internet: 'no',
        files: 'yes, a client handout in your output folder (if you ask for one)',
        does: 'Scores the CDC Prediabetes Risk Test, checks whether the person meets CDC’s rules for the National Diabetes Prevention Program lifestyle change program, and lists the Massachusetts program sites that the Department of Public Health publishes for a town, county, or the whole state. It explains in-person, live online, and on-demand programs, and Medicare’s diabetes prevention program. It can write a plain-language handout for the client. It is honest about what it could not check: CDC’s national registry could not be reached, so it does not say which sites are CDC-recognized or how each one is delivered, and it does not list online providers. It points you to CDC’s finder for those.',
        when: [
          'Screening at a health fair or senior center.',
          'A client has a high-risk result or prediabetes and wants a program near home.',
          'Counting how many people at an event scored high risk.',
        ],
        provide: [
          'For a score: age, sex (man or woman), family history, high blood pressure, whether the person is physically active, and height and weight (or BMI). For eligibility, also whether the person has diabetes or is pregnant, and any blood test result.',
          'A town or county for the program list. Optionally a screening-event file in your data folder.',
        ],
        examples: [
          'A 54-year-old man, 5 feet 10 and 240 pounds, has a family history, high blood pressure, and is not active. What is his risk test score, and are there programs in Fall River?',
          'Is a 45-year-old Asian American woman with a BMI of 24 and a past gestational diabetes diagnosis eligible for the lifestyle change program?',
          'Count how many people scored high risk in health_fair_responses.csv.',
        ],
        returns: [
          'The score with points for each factor, the eligibility check (met, not met, or unknown for each rule), program sites near the area with address and organization phone, how to find online and distance options, Medicare program facts, a statement on MassHealth, and an optional handout with its reading grade.',
        ],
        notes: [
          'A risk score is not a diagnosis: only a blood test can diagnose prediabetes.',
          'The site list is DPH’s and shows no date, so call a site before you refer. Names of coordinators are not kept, only organization phone numbers where DPH lists them.',
          'No source was found for MassHealth coverage of the program, so the tool makes no statement about it. Ask MassHealth or the member’s plan.',
          'A screening-event file is summarized as counts only, with small counts hidden.',
        ],
      },
      // End of Phase 15 screen tools.
      // Phase 15 prevent (CHR-04, CHR-05) tools.
      {
        id: 'preventive_services_lookup',
        title: 'Look up recommended preventive services',
        toolset: 'chronic',
        internet: 'optional',
        files: 'optional, a plain-language client handout in your output folder',
        does: 'Lists the U.S. Preventive Services Task Force (USPSTF) grade A and B recommendations that fit a person’s age, sex at birth, pregnancy status, and risk factors. Each one shows its grade, the date of the Task Force statement, the Task Force’s own wording of who it is for, what you still need to check, and a plain-language summary you can use with the client. It also shows Massachusetts options: the Massachusetts Breast and Cervical Cancer Program, the colorectal cancer program, and the 1-800-QUIT-NOW quit line.',
        when: [
          'Planning a screening conversation or a well-person visit.',
          'Finding out what a client in a certain age group may be due for.',
          'Looking for a free or low-cost breast or cervical cancer screening program for an uninsured client.',
        ],
        provide: [
          'Age in years and sex assigned at birth. Never a name, date of birth, or other identifier.',
          'Pregnancy status (pregnant, postpartum, or not pregnant), if it applies.',
          'Any risk factors you know: tobacco use and pack-years, BMI, blood pressure or cholesterol problems, 10-year heart disease risk, and where you judge the person to be at higher risk (for example HIV, hepatitis B, or TB).',
        ],
        examples: [
          'What USPSTF A and B screenings apply to a 52-year-old woman who smoked 25 pack-years and quit 8 years ago?',
          'A 29-year-old woman is 20 weeks pregnant. Which recommended preventive services should I cover, and give me client summaries.',
          'My client is 47, has no health insurance, and needs a mammogram. What programs in Massachusetts can help?',
        ],
        returns: [
          'Recommendations that apply, and a separate list of those that depend on a risk factor you did not give, with the grade, the statement date, the Task Force’s wording, a client summary, and links.',
          'Massachusetts program options with how DPH describes who can use them, and, when you are online, the federal MyHealthfinder list for the same age and sex.',
        ],
        notes: [
          'This is the grade A and B list only. It is not the full list of recommended services: vaccines (CDC’s immunization schedule) and grade C recommendations are separate.',
          'The recommendations were read from the Task Force’s published statements because its web site could not be reached; the answer shows each statement’s date, and you should check the Task Force’s page for any newer version.',
          'The tool checks only age, sex, pregnancy, and the risk factors you give. The rest of each recommendation’s population (for example “average risk”) is shown for you to apply.',
          'The content is a draft that the subject matter expert has not yet reviewed.',
        ],
      },
      {
        id: 'asthma_home_assessment',
        title: 'Plan an asthma home visit',
        toolset: 'chronic',
        internet: 'no',
        files: 'yes, blank forms and filled asthma action plans in your output folder',
        does: 'Gives you what you need for a home-based asthma visit: the CDC, EPA, and HUD home asthma trigger checklist with an action step for each answer, DPH’s asthma control categories (and it can sort the answers you give into well controlled, not well controlled, or very poorly controlled), the NHLBI asthma action plan form, and the home asthma visiting programs DPH lists in Massachusetts. For mold, moisture, pests, and heat it points you to the state housing code sections already in the housing tool.',
        when: [
          'Preparing for a home visit with a child or adult whose asthma is not controlled.',
          'Helping a family ask a landlord for repairs that affect asthma.',
          'Finding a community health worker asthma home visiting program to refer to.',
          'Making asthma action plans for a school or clinic from your own list.',
        ],
        provide: [
          'Which parts you want (checklist, control, action plan, services). The default is all four.',
          'For services: the town.',
          'For the control categories: counts only, such as days with symptoms in the past 30 days, nights woken, rescue inhaler uses per day, and how much asthma limited activities. No names.',
          'To fill action plans: a spreadsheet in your data folder with one row per person and the prescriber’s plan details (the doctor, phone numbers, best peak flow, medicines and doses).',
        ],
        examples: [
          'Give me the home asthma trigger checklist and a blank asthma action plan.',
          'A 9-year-old had symptoms on 12 of the last 30 days and woke 4 nights. What control category is that?',
          'What asthma home visiting programs are there near Springfield?',
          'Fill asthma action plans from my asthma_plans.xlsx file.',
        ],
        returns: [
          'The checklist by section, the control table and the category of your answers, the action plan template with peak flow zones, and the home visiting programs with how each fits your town.',
          'A blank checklist and a blank action plan as Word files, and filled plans in one file in your output folder.',
        ],
        notes: [
          'The control categories are a screening table from the Massachusetts DPH adapted from NHLBI’s national guidelines (the guidelines themselves could not be reached). They are not a treatment decision. For children under 12 they are only a rough screen.',
          'The Asthma Control Test is a licensed form and is not reproduced.',
          'The tool never chooses medicines or doses. They appear in a filled plan only if they are in your file, and the prescriber should check the plan.',
          'DPH’s program directory lists regions, not towns, and no phone numbers; call the program to confirm who it serves.',
          'Names and details from your file go only into the plans in your output folder; the chat sees counts.',
        ],
      },
      // End of Phase 15 prevent tools.
      // Phase 15 cha (EQ-08, EQ-09, EQ-10) tools.
      {
        id: 'community_health_assessment_builder',
        title: 'Build a community health assessment (MAPP 2.0)',
        toolset: 'equity',
        internet: 'optional',
        files: 'yes, a draft report in your output folder; reads hospital CHNA reports and a partner list from your data folder if you give them',
        does: 'Guides a community health assessment the way MAPP 2.0 (from NACCHO) lays it out, for a town or county. It gathers the numbers for the community status assessment (the community profile with CDC chronic disease estimates and social vulnerability, language access, environmental justice, climate, food access, and water), gives you a checklist of partner sectors to cover and prompts for the community context assessment, and lists the hospitals near you with the rules for their community health needs assessments (CHNAs) and where to find them. If you save hospitals’ CHNA reports in your data folder, it pulls out each report’s priority list with the page number. It drafts report sections as a Word document.',
        when: [
          'Starting or updating your town’s community health assessment.',
          'You want to know what nearby hospitals found and prioritized in their CHNAs.',
          'Checking whether your partner list covers the sectors that matter.',
        ],
        provide: [
          'The town or county.',
          'Optionally the hospitals you want to follow, a folder of their CHNA reports (PDF or Word) in your data folder, and a partner list (a spreadsheet with organization and sector).',
        ],
        examples: [
          'Start a community health assessment for Lowell.',
          'Read the CHNAs in /data/chna and tell me what Lowell General and Lawrence General prioritized.',
          'Count my partner list by sector and tell me who is missing.',
        ],
        returns: [
          'The MAPP 2.0 phases and assessments, the data sections (each with its sources and any section that could not be built), the partner checklist, hospitals with CMS type and ownership, the federal and Massachusetts CHNA rules, links to find CHNAs, the priorities found in your CHNA files with page numbers and shared themes, next steps, and a draft report in your output folder.',
        ],
        notes: [
          'MAPP 2.0 was checked against journal articles because NACCHO’s own pages could not be opened. Check NACCHO’s handbook before you cite a phase or tool by name.',
          'This is a draft from public data. It does not replace what your partners and residents tell you.',
          'Priority lists are found by pattern and can be incomplete. Check each against the report.',
          'Only organization names are read from a partner list; contact people are never read. A list of clients is refused, and so is a CHNA file that looks like client records.',
          'The content is waiting for your review.',
        ],
      },
      {
        id: 'prioritize_health_issues',
        title: 'Rank community health issues (Hanlon, PEARL, nominal group)',
        toolset: 'equity',
        internet: 'no',
        files: 'yes, a record of the method and rationale in your output folder',
        does: 'Ranks the community health issues you give it. For each issue you rate the size of the problem, how serious it is, and how effective an intervention could be, and answer the five PEARL questions (propriety, economics, acceptability, resources, legality). The tool does the arithmetic with the Hanlon method, shows ties and missing ratings openly instead of hiding them, places each issue on a grid of importance and feasibility, and can score a group vote from totals. It writes the method, the scores, and your reasons to a document.',
        when: [
          'Your partners need to narrow a long list of health issues to a few priorities.',
          'You want a record of how priorities were chosen, for the assessment report or a hospital partner.',
          'Scoring a nominal group or multivoting session.',
        ],
        provide: [
          'The issues (community issues, not people) with size 0 to 10, seriousness 0 to 20 (or urgency, severity, economic loss, and effect on others, 0 to 5 each), effectiveness 0 to 10, and the five PEARL answers.',
          'Your reasons for each rating, so the record shows how it was decided.',
          'Optionally the vote totals: how many people gave each issue first, second, third place (never names).',
        ],
        examples: [
          'Rank these five issues for our community: housing cost, food insecurity, transportation, opioid overdose, and dental care. Here are my ratings.',
          'We held a nominal group with 12 people who each ranked their top 3. Score the vote.',
          'Screen out any issue that fails PEARL before ranking.',
        ],
        returns: [
          'The ranking with scores, ties, issues still missing ratings, close calls, the strategy grid, the vote scores if given, and a document with the method and your reasons.',
        ],
        notes: [
          'The scores are only as good as the ratings. Rate size from your data and write down why.',
          'Ties share a rank and are not broken for you. Missing ratings are not counted as zero.',
          'The Hanlon formula comes from published papers; NACCHO’s own worksheet could not be opened, so check it matches the worksheet your partners use.',
          'The importance and feasibility grid is this project’s own design.',
          'Type community issues, not names: everything you type goes to the AI service.',
        ],
      },
      {
        id: 'analyze_qualitative_data',
        title: 'Analyze focus group and interview transcripts',
        toolset: 'equity',
        internet: 'no',
        files: 'yes, reads transcripts and writes de-identified results to your output folder',
        does: 'Helps you analyze focus groups and key informant interviews you have saved as transcripts. It reads them only on your computer, removes names, organizations, places, phone numbers, addresses, dates, and similar details (speakers become P1, P2, and so on), and codes what people said with the codes you supply or a draft starter list. What comes back to the chat is counts only: how many different people raised each theme. A theme that fewer than three people raised is held back so no one can be picked out. The de-identified excerpts under each code and a theme summary go to your output folder.',
        when: [
          'You held focus groups or interviews for a community assessment and need the themes.',
          'You want a coded table of excerpts you can read, check, and recode.',
          'You need to share findings without sharing what any one person said.',
        ],
        provide: [
          'The transcript files (or a folder of them) in your data folder, with speaker turns written as “Name: what they said”.',
          'Optionally your own codebook (a spreadsheet with code, definition, and keywords) and a list of the names that appear, so they are removed everywhere.',
        ],
        examples: [
          'Analyze the transcripts in /data/focus_groups with my codebook.',
          'What themes came up in the two interviews? Remove the names first.',
          'Make a coded excerpt table I can use for my report.',
        ],
        returns: [
          'A count of what was removed, the codebook, for each theme how many participants raised it and in how many transcripts, words that came up often but fit no code, and the files in your output folder.',
        ],
        notes: [
          'Nothing a person said, no speaker name, and no file name comes back to the chat.',
          'Coding by keyword is a rough first pass: it finds words, not meaning. Read the excerpts yourself and recode by hand before you report a finding.',
          'Removing names by pattern is not perfect. Read the de-identified files before you share them; they can still hold a detail that points to someone.',
          'The starter codebook is a draft written for this tool. Use your own for a real analysis.',
          'Scanned PDFs have no text the tool can read.',
        ],
      },
      // End of Phase 15 cha tools.
      // Phase 15 geo (EPI-13) tools.
      {
        id: 'geocode_and_aggregate_addresses',
        title: 'Count addresses by town or census tract',
        toolset: 'epi',
        internet: 'no',
        files: 'optional, a file of each row with its location in your output folder',
        does: 'Finds where the addresses in your own list are, without sending them anywhere, and gives you only counts: by town, by census tract, or by block group. It matches each address to MassGIS address points and Census street ranges inside the server, tells you how many addresses it could place exactly, by interpolation, by street only, or by ZIP code only, and hides small counts. The assistant never sees an address.',
        when: [
          'Mapping where cases, clients, or program participants live for a community health assessment or a grant.',
          'Checking whether a cluster of cases falls in one neighborhood before you ask for more data.',
          'Turning a list of home addresses into counts by census tract so you can match them to census or social vulnerability data.',
        ],
        provide: [
          'A spreadsheet or CSV in your data folder with an address column (and a town and ZIP column if you have them). Other columns such as names or phone numbers can stay in the file; they are not read.',
          'Optionally the level you want (town, tract, or block group) and whether you want a file with each row\'s location saved to your output folder.',
        ],
        examples: [
          'Count the addresses in pertussis_cases.csv by census tract.',
          'Which towns do the clients in my program list live in? Use the ZIP column if there is no town column.',
          'Geocode the home visiting list and save the locations to a file for my own mapping, but only show me counts.',
        ],
        returns: [
          'How many addresses were located at each level of precision and the match rate, then counts by town, tract, or block group with small counts shown as *.',
          'Optionally a file in your output folder with each row\'s match level, town, tract, block group, and latitude and longitude. That file contains addresses, so it stays in your secure folder.',
        ],
        notes: [
          'It works without the full street index: it counts by town from a town column, a town written in the address, or a ZIP code. Street-level matching and tract counts need the full index (about 35 MB, built once from MassGIS and Census files; see the technical notes), and four towns (Alford, Amherst, Northampton, and Pelham) are included as a sample.',
          'ZIP codes cross town lines, so counts from ZIP codes alone are approximate. The tool only assigns a ZIP code to a town when that town holds at least 90 percent of the ZIP code\'s addresses.',
          'Boston addresses are placed from Census street ranges only because MassGIS does not publish Boston\'s address points.',
          'A column of names, birth dates, phone numbers, or ID numbers will not be used as the address column, and nothing from your file is repeated back.',
          'Counts are numbers of addresses, not people or households. Divide by a population from the census to get rates.',
        ],
      },
      // End of Phase 15 geo tools.
    ],
  },
  // End of Phase 15 chapter.
  // Phase 16 chapter goes below this line.
  {
    title: 'Program planning, evaluation, and evidence',
    intro: 'TODO: coordinator writes the chapter introduction at the end of the phase.',
    tools: [
      // Phase 16 design (PLAN-01, PLAN-02, PLAN-05) tools.
      // End of Phase 16 design tools.
      // Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11) tools.
      // End of Phase 16 interventions tools.
      // Phase 16 appraise (EBP-03, EBP-04) tools.
      {
        id: 'appraise_evidence',
        title: 'Appraise a study',
        toolset: 'ebp',
        internet: 'optional',
        files: 'yes, can read a study file you choose and writes a Word record of your answers to your output folder',
        does: 'Walks you through a critical appraisal of one study. It suggests the study design from the abstract or methods you paste (you confirm it), then lists the appraisal questions for that design, each with its source: risk-of-bias domains for randomized trials and for non-randomized program evaluations, reporting and bias prompts for cohort, case-control, and cross-sectional studies, the 16 AMSTAR 2 items for systematic reviews, questions for qualitative studies, and prompts for quality improvement reports. You answer the questions; the tool records your answers, points out the ones to look at again, and applies each instrument’s own rule for an overall rating where there is one. For a group of studies on one outcome it adds up your GRADE judgments to a certainty level, or records your GRADE-CERQual judgments for a qualitative finding. If you give a DOI it can fetch the citation from Crossref.',
        when: [
          'You are deciding whether a study is good enough to use in a program or a policy.',
          'You are appraising articles for a project, a journal club, or an evidence review.',
          'You want a written record of how you judged a study.',
        ],
        provide: [
          'The abstract or methods section pasted in, or a study file in your data folder (not a client record), or the DOI.',
          'Later, your answers to the questions, using the question names the tool gives you.',
          'For GRADE: the outcome and your judgment of each domain.',
        ],
        examples: [
          'Here is the abstract of a school asthma trial. What kind of study is it and what should I check?',
          'Appraise this systematic review. I answered yes to the PICO question, no to the protocol question, and yes to the rest.',
          'I have four cohort studies on home visiting and low birth weight. Help me rate the certainty with GRADE: serious risk of bias, no other concerns.',
        ],
        returns: [
          'The suggested design with the words that pointed to it, the questions with their sources, your answers, the ones to look at again, the instrument’s overall rating, a GRADE or CERQual summary, a link to CASP, and a Word record.',
        ],
        notes: [
          'The tool never answers a question for you. Appraisal is a judgment, and GRADE ratings are not calculations.',
          'The questions are not the CASP checklists. They were written from open-access papers; CASP is linked, and its license terms could not be checked, so read them on the checklist you download.',
          'The design suggestion is a guess from words. Confirm it before relying on the questions.',
          'A file that looks like a client record (Social Security number, birth date, medical record number) is refused. The study text is never repeated back.',
          'Offline, the DOI is used as you typed it and the tool says so.',
          'The content is waiting for your review. Some parts come from journal abstracts only; the notes say which.',
        ],
      },
      {
        id: 'build_evidence_synthesis_table',
        title: 'Build an evidence table and PRISMA flow diagram',
        toolset: 'ebp',
        internet: 'no',
        files: 'yes, writes Word and Excel files and the diagram to your output folder',
        does: 'Turns your list of appraised studies into a synthesis table (study, design, population, sample size, intervention, outcome, effect, quality rating) with a summary of how many studies there are by design and by quality rating. The Excel file has live formulas, so the totals update when you edit the table. From the counts you enter it also draws a PRISMA 2020 flow diagram, as an accessible picture with a written description and as Mermaid text, and it checks that the numbers add up. If they do not, it tells you exactly which do not.',
        when: [
          'Writing up an evidence review for a project, a thesis, or a program proposal.',
          'Keeping a running table of the studies you have appraised.',
          'You need a PRISMA flow diagram and want the arithmetic checked.',
        ],
        provide: [
          'Your studies: how you cite each one, design, population, sample size, intervention, outcome, effect as reported, and your quality rating and the tool it came from.',
          'For the diagram: records identified, duplicates removed, records screened and excluded, reports assessed, reports excluded with reasons, and studies included.',
        ],
        examples: [
          'Build a synthesis table from these six studies and give me an Excel version.',
          'Draw a PRISMA flow diagram: 1,333 records, 241 duplicates removed, 1,092 screened, 1,050 excluded, 42 reports sought, 2 not retrieved, 40 assessed, 34 excluded for the reasons below, 6 included.',
        ],
        returns: [
          'The table, the summary, the checked flow counts, the diagram as text, SVG, and Mermaid, and Word and Excel files in your output folder.',
        ],
        notes: [
          'Everything in the table is what you typed. The tool does not look up, rate, or summarize studies.',
          'Quality ratings from different tools are counted separately, because “low” means different things in different tools.',
          'The diagram follows the written PRISMA 2020 guidance, not the official picture, which could not be read. For a publication, use the official template at the PRISMA website.',
          'The content is waiting for your review.',
        ],
      },
      // End of Phase 16 appraise tools.
      // Phase 16 qi (EBP-05) tools.
      {
        id: 'plan_quality_improvement',
        title: 'Plan a quality improvement project',
        toolset: 'ebp',
        internet: 'no',
        files: 'yes, a plan (Word), a tracker (Excel), diagrams, and a chart in your output folder; reads your measure file if you give one',
        does: 'Walks you through a quality improvement project with the Model for Improvement. It turns what you tell it into an aim statement (how much, for whom, by when) with checks, a set of outcome, process, and balancing measures, a driver diagram, a cause-and-effect (fishbone) diagram, a process map, PDSA cycle plans, and a checklist for writing it up with the SQUIRE 2.0 guideline. If you give it your measure numbers, it draws a run chart or control chart and tells you whether the pattern looks like real change, using the rules taught by IHI and a second rule set that did better in published testing. It writes a Word plan and an Excel tracker whose formulas keep checking the rules as you add each month.',
        when: [
          'Starting a quality improvement project for a clinic, school health office, or program (a capstone or practicum project, for example).',
          'Checking whether a change you made moved your measure, from monthly numbers.',
          'Preparing to write up an improvement project for a journal or a meeting.',
        ],
        provide: [
          'Whatever you have: the aim (what is changing, for whom, from what number to what number, by when), your measures, the drivers and change ideas your team came up with, the steps of the process, and the small tests you plan. Leave anything out and the tool gives you a blank scaffold and tells you what is missing.',
          'Optionally your measure data: numbers typed in, a file with one row per visit or event and a date column, or a file with one row per month.',
        ],
        examples: [
          'Help me plan a project to raise on-time vaccination at our clinic from 68 percent to 80 percent by June 2027.',
          'Here are our monthly counts, with the reminder letters starting in October. Did anything change?',
          'Make a driver diagram and a fishbone for late immunization records, and give me the SQUIRE checklist.',
        ],
        returns: [
          'The aim statement and what is still missing from it, the measures, the three diagrams (as Mermaid text, pictures with descriptions, and outlines), the PDSA plans with checks, the SQUIRE items, and any run chart or control chart signals with a plain-language reading.',
          'A Word plan, an Excel tracker, the diagrams, and the chart in your output folder.',
        ],
        notes: [
          'Only totals by month or week are shown. Counts under your suppression threshold are hidden, and then the median and limits are hidden too, because they would give the hidden number away. Your output folder keeps everything.',
          'A signal means the pattern is unlikely to be chance, not why it happened. The tool shows both rule sets because the IHI rules (6 in a row on one side, 5 in a row going one way) give more false alarms than the other set; follow your course or agency if it names a rule set.',
          'Use roles, not names, for who owns a measure, and keep names and record numbers out of the tracker.',
          'The wording comes from open-access articles; IHI\'s own pages could not be reached, and the SQUIRE guideline is copyrighted (the tool paraphrases and links to it). The content is waiting for your review.',
          'It does not decide whether a project is quality improvement or research. Ask your agency or IRB.',
        ],
      },
      // End of Phase 16 qi tools.
    ],
  },
  // End of Phase 16 chapter.
  // Phase 17 chapter goes below this line.
  {
    title: 'Grants and finance',
    intro: 'TODO: coordinator writes the chapter introduction at the end of the phase.',
    tools: [
      // Phase 17 grants (PLAN-06, PLAN-07, PLAN-08) tools.
      // End of Phase 17 grants tools.
      // Phase 17 budget (PLAN-09, PLAN-10) tools.
      // End of Phase 17 budget tools.
      // Phase 17 finance (LEAD-01, LEAD-02) tools.
      // End of Phase 17 finance tools.
    ],
  },
  // End of Phase 17 chapter.
  // Phase 18 chapter goes below this line.
  {
    title: 'Leadership, accreditation, and professional practice',
    intro: 'TODO: coordinator writes the chapter introduction at the end of the phase.',
    tools: [
      // Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05) tools.
      // End of Phase 18 strategy tools.
      // Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08) tools.
      // End of Phase 18 workforce tools.
      // Phase 18 ethics (EBP-07, POL-05, POL-06) tools.
      // End of Phase 18 ethics tools.
    ],
  },
  // End of Phase 18 chapter.
  // Phase 19 chapter goes below this line.
  {
    title: 'Policy and law',
    intro: 'TODO: coordinator writes the chapter introduction at the end of the phase.',
    tools: [
      // Phase 19 tracking (POL-01, POL-03) tools.
      // End of Phase 19 tracking tools.
      // Phase 19 law (POL-02, POL-04, POL-07) tools.
      // End of Phase 19 law tools.
    ],
  },
  // End of Phase 19 chapter.
];

// ---------------------------------------------------------------------------------------
// Appendices
// ---------------------------------------------------------------------------------------

const glossary = [
  ['Aggregate data', 'Counts, rates, and other summaries, as opposed to records about individual people.'],
  ['American Community Survey (ACS)', 'The Census Bureau’s yearly survey. Five-year estimates are used for towns and tracts, and each comes with a margin of error.'],
  ['Cache', 'A recent copy of public data saved on your computer so the server can reuse it without contacting the source again.'],
  ['CDSi', 'CDC’s Clinical Decision Support for Immunization: published logic and data for applying the ACIP schedule to a person’s vaccination history.'],
  ['Complementary suppression', 'Hiding additional table cells so that a suppressed small number cannot be recalculated from the totals.'],
  ['Container', 'A sealed package that holds a program and everything it needs. The server runs in one on your computer, using Docker Desktop or a similar program.'],
  ['CVX, MVX, NDC', 'Codes for a vaccine (CVX), its manufacturer (MVX), and a specific package (National Drug Code).'],
  ['Data folder', 'The one folder on your computer that the server may read (this guide calls it PHN-Data). Files the server creates go in its output subfolder.'],
  ['Decision support', 'Information to support your professional judgment, not a replacement for it.'],
  ['HAN', 'CDC’s Health Alert Network notices.'],
  ['HMCC', 'Health and Medical Coordinating Coalition: Massachusetts’ regional health care preparedness coalitions.'],
  ['Limited data set', 'Data with direct identifiers removed but some dates and geography kept. Still protected health information under HIPAA.'],
  ['Margin of error (MOE)', 'The range around a survey estimate within which the true value probably lies (90% confidence for ACS).'],
  ['MCP', 'Model Context Protocol: the open standard that lets an AI assistant use outside tools such as this server.'],
  ['MMWR', 'CDC’s Morbidity and Mortality Weekly Report.'],
  ['NNDSS', 'CDC’s National Notifiable Diseases Surveillance System.'],
  ['PLACES', 'CDC’s model-based local estimates of chronic disease and related measures.'],
  ['Profile', 'A named set of tool groups suited to a role, such as school_nurse.'],
  ['Review status', 'Whether bundled reference content has been checked by the project’s subject matter expert (“reviewed”) or not yet (“pending”).'],
  ['Row-level (record-level) data', 'Data with one row per person or event. The server does not send it to the AI by default.'],
  ['Safe Harbor', 'The HIPAA de-identification method that removes 18 kinds of identifiers.'],
  ['Small-cell suppression', 'Hiding small counts (1 to 4 with the default threshold of 5) in published tables to protect privacy.'],
  ['Snapshot', 'A dated copy of public data bundled with the server so tools work offline.'],
  ['SVI', 'The CDC/ATSDR Social Vulnerability Index.'],
  ['Tool', 'A single capability the AI assistant can use, such as calculating isolation dates.'],
  ['Tool group (toolset)', 'A set of related tools, such as cd (communicable disease), that can be turned on or off.'],
  ['VIS', 'Vaccine Information Statement.'],
  ['Wastewater viral activity level (WVAL)', 'CDC’s category for how much virus is found in wastewater at a sampling site.'],
];

const troubleshooting = [
  ['A tool I need is not available.', 'Your profile does not include its tool group. Ask Claude to turn on the group (for example “Turn on the ebp toolset”) or change your profile (section 2.5). If the profile is set in the Claude Desktop configuration file, change it there or ask IT.'],
  ['The tool cannot find my file.', 'Make sure the file is inside your data folder, and use its name relative to that folder. Ask “What files are in my data folder?”'],
  ['The answer says it used a snapshot instead of live data.', 'The source could not be reached, often because of an agency firewall or a missing optional key. The answer is still valid as of the snapshot’s date. IT can review the network settings, proxy, certificate bundle, and optional API keys described in the README.'],
  ['A data tool refused my file.', 'Tools that expect counts refuse files with identifiers or one row per person. Use a tool built for record-level files (such as the outbreak analysis or roster checks), or summarize the data first.'],
  ['I think an answer is wrong.', 'Ask for the method, sources, and caveats, and check the data dates with the data source status tool. If the content is wrong, report it so the subject matter expert can correct it.'],
  ['IT needs to check the installation.', 'The README describes a “check” command that prints the configuration and verifies the bundled data.'],
];

module.exports = {
  VERSION,
  GUIDE_DATE,
  TOOL_COUNT,
  TOOLSETS,
  INTERNET,
  frontChapters,
  toolChapters,
  glossary,
  troubleshooting,
};
