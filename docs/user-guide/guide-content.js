// Content of the user guide (docs/user-guide/PHN-MCP-Server-User-Guide.docx).
//
// Edit this file to change what the guide says; build-user-guide.js turns it into Word.
// Inline markup: **bold** and `code`. Use curly quotes and real dashes in prose.
//
// Only tools that are implemented and registered in crates/phn-server belong here. When a
// tool is added, add it to the right chapter below and to the counts in the front matter,
// then rebuild (see README.md in this folder).

'use strict';

const VERSION = '0.7.0';
const GUIDE_DATE = 'November 2026';
const TOOL_COUNT = 48;

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
          { p: `The ${TOOL_COUNT} tools fall into seven work areas. Chapters 4 to 10 describe each tool.` },
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
                // End of Phase 7 work-area rows.
                // Phase 8 work-area rows go below this line.
                // End of Phase 8 work-area rows.
                // Phase 9 work-area rows go below this line.
                // End of Phase 9 work-area rows.
                // Phase 10 work-area rows go below this line.
                // End of Phase 10 work-area rows.
                // Phase 11 work-area rows go below this line.
                ['Substance use, crisis resources, and health communication', 'Check reading level and plain language.', '1'],
                // End of Phase 11 work-area rows.
                // Phase 12 work-area rows go below this line.
                // End of Phase 12 work-area rows.
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
                ['school_nurse', 'School nurses', 'core, vax, cd, epi, comms', 'equity, chronic, ebp'],
                ['cd_nurse', 'Communicable disease and epidemiology nurses at a board of health', 'core, cd, vax, epi, comms, equity', 'chronic, ebp'],
                ['phn_leader', 'Public health nurse leaders and directors', 'core, epi, equity, comms, ebp, chronic', 'cd, vax'],
                ['preparedness', 'Emergency preparedness coordinators', 'core, epi, equity, comms, cd', 'vax, chronic, ebp'],
                ['(none)', 'Anyone who wants every tool', 'All groups', '—'],
              ],
            },
          },
          {
            p: 'Each tool description in this guide names its group. If a tool you need is not available, ask Claude to change your profile or turn on the group, for example “Turn on the ebp toolset.” If your profile was set in the Claude Desktop configuration file, that setting takes priority; the tool will say so, and you or IT can change the file. Profiles also switch on groups reserved for future versions; those have no effect yet.',
          },
        ],
      },
      {
        title: 'Where the information comes from',
        blocks: [
          {
            bullets: [
              '**Bundled reference content**, drafted from 105 CMR 300.000, DPH’s disease guide and guidance, DPH’s school immunization requirements, CDC and ACIP, AAP, ACOG, AAFP, Immunize.org, and EPA, each entry with its citation and a confidence level. All of it is pending expert review.',
              '**Dated data snapshots** bundled with the server: American Community Survey estimates, the Social Vulnerability Index, CDC PLACES, map boundaries, DPH’s school immunization survey, CDC immunization coverage estimates, an offline copy of DPH’s disease guide and fact sheets, and recent CDC surveillance data.',
              '**Live public data**, when the internet is available: the Census Bureau, CDC’s data portal (data.cdc.gov), PubMed, CDC’s MMWR and Health Alert Network, RxNorm, openFDA, and DailyMed.',
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
              '**Give exact dates.** Tools count days from the dates you give. “Onset October 20, 2026” is better than “about two weeks ago.”',
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
// Tool chapters: chapters 4–10. One entry per implemented tool.
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
  // End of Phase 7 chapter.
  // Phase 8 chapter goes below this line.
  // End of Phase 8 chapter.
  // Phase 9 chapter goes below this line.
  // End of Phase 9 chapter.
  // Phase 10 chapter goes below this line.
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
        ],
      },
    ],
  },
  // End of Phase 11 chapter.
  // Phase 12 chapter goes below this line.
  // End of Phase 12 chapter.
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
