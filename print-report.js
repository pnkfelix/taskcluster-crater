'use strict';

var debug = require('debug')(__filename.slice(__dirname.length + 1));
var reports = require('./reports');
var util = require('./crater-util');
var dist = require('./rust-dist');
var crateIndex = require('./crate-index');
var db = require('./crater-db');
var fs = require('fs');

function main() {
  var reportSpec = getReportSpecFromArgs();
  if (!reportSpec) {
    console.log("can't parse report spec");
    process.exit(1);
  }

  var config = util.loadDefaultConfig();
  var dbCredentials = config.dbCredentials;

  if (reportSpec.type == "current") {
    var date = reportSpec.date;
    reports.createCurrentReport(date, config).then(function(report) {
      console.log("# Current Report");
      console.log();
      console.log("* current stable is " + report.stable);
      console.log("* current beta is " + report.beta);
      console.log("* current nightly is " + report.nightly);
    }).done();
  } else if (reportSpec.type == "weekly") {
    var date = reportSpec.date;
    db.connect(config).then(function(dbctx) {
      var p = reports.createWeeklyReport(date, dbctx, config);
      return p.then(function(report) {
	console.log("# Weekly Report");
	console.log();
	console.log("Date: " + report.date);
	console.log();
	console.log("## Current releases");
	console.log();
	console.log("* The most recent stable release is " + report.currentReport.stable + ".");
	console.log("* The most recent beta release is " + report.currentReport.beta + ".");
	console.log("* The most recent nightly release is " + report.currentReport.nightly + ".");
	console.log();
	console.log("## Regressions");
	console.log();
	console.log("* There are currently " + report.betaRootRegressions.length +
		    " root regressions from stable to beta.");
	console.log("* There are currently " + report.nightlyRootRegressions.length +
		    " root regressions from beta to nightly.");
	console.log("* There are currently " + report.betaRegressions.length +
		    " regressions from stable to beta.");
	console.log("* There are currently " + report.nightlyRegressions.length +
		    " regressions from beta to nightly.");
	console.log();
	console.log("## Coverage");
	console.log();
	console.log("From stable to beta:");
	console.log("* " + report.betaStatuses.length + " crates tested: " +
		    report.betaStatusSummary.working + " working / " +
		    report.betaStatusSummary.notWorking + " not working / " +
		    report.betaStatusSummary.regressed + " regressed / " +
		    report.betaStatusSummary.fixed + " fixed.");
	console.log();
	console.log("From beta to nightly:");
	console.log("* " + report.nightlyStatuses.length + " crates tested: " +
		    report.nightlyStatusSummary.working + " working / " +
		    report.nightlyStatusSummary.notWorking + " not working / " +
		    report.nightlyStatusSummary.regressed + " regressed / " +
		    report.nightlyStatusSummary.fixed + " fixed.");
	console.log();
	console.log("## Beta root regressions, (unsorted):");
	console.log();
	report.betaRootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
	console.log();
	console.log("## Nightly root regressions, (unsorted):");
	console.log();
	report.nightlyRootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
	console.log();
	console.log("## Beta non-root regressions, (unsorted):");
	console.log();
	report.betaNonRootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
	console.log();
	console.log("## Nightly non-root regressions, (unsorted):");
	console.log();
	report.nightlyNonRootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
      }).then(function() {
	return db.disconnect(dbctx);
      });
    }).done();
  } else if (reportSpec.type == "comparison") {
    var date = reportSpec.date;
    db.connect(config).then(function(dbctx) {
      var p = reports.createComparisonReport(reportSpec.fromToolchain, reportSpec.toToolchain,
					     dbctx, config);
      return p.then(function(report) {
	console.log("# Comparison report");
	console.log();
	console.log("* From: " + util.toolchainToString(report.fromToolchain));
	console.log("* To: " + util.toolchainToString(report.toToolchain));
	console.log();
	console.log("## Regressions");
	console.log();
	console.log("* There are " + report.rootRegressions.length + " root regressions");
	console.log("* There are currently " + report.regressions.length + " regressions");
	console.log();
	console.log("## Coverage");
	console.log();
	console.log("* " + report.statuses.length + " crates tested: " +
		    report.statusSummary.working + " working / " +
		    report.statusSummary.notWorking + " not working / " +
		    report.statusSummary.regressed + " regressed / " +
		    report.statusSummary.fixed + " fixed.");
	console.log();
	console.log("## Root regressions, (unsorted):");
	console.log();
	report.rootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
	console.log();
	console.log("## Non-root regressions, (unsorted):");
	console.log();
	report.nonRootRegressions.forEach(function(reg) {
	  var link = reg.inspectorLink;
	  console.log("* [" + reg.crateName + "-" + reg.crateVers + "](" + link + ")");
	});
      }).then(function() {
	return db.disconnect(dbctx);
      });
    }).done();
  }
}

function getReportSpecFromArgs() {
  if (process.argv[2] == "current") {
    return {
      type: "current",
      date: process.argv[3] || util.rustDate(new Date(Date.now()))
    };
  } else if (process.argv[2] == "weekly") {
    return {
      type: "weekly",
      date: process.argv[3] || util.rustDate(new Date(Date.now()))
    };
  } else if (process.argv[2] == "comparison") {
    if (!process.argv[3] || !process.argv[4]) {
      return null;
    }
    return {
      type: "comparison",
      fromToolchain: util.parseToolchain(process.argv[3]),
      toToolchain: util.parseToolchain(process.argv[4])
    };
  } else {
    return null;
  }
}

main();
