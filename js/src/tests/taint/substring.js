/* -*- tab-width: 2; indent-tabs-mode: nil; js-indent-level: 2 -*- */


/**
   Filename:     basicfunctions.js
   Description:  'This tests the new basic tainted functions of strings.'

   Author:       Stephan Pfistner
*/

var SECTION = 'no section';
var VERSION = 'no version';
startTest();
var TITLE = 'Taint:op:substring';


var tainted = "is it" + String.newAllTainted("tainted");
assertEq(tainted.taint.length, 1);

//substring
// full non taint
assertEq(tainted.substring(0,5).taint.length, 0);
// full taint
var fulltaint=tainted.substring(5,10).taint;
assertEq(fulltaint.length, 1);
assertEq(fulltaint[0].begin, 5); //substring has absolute indices for start
assertEq(fulltaint[0].end, 10); // .. and end
assertEq(fulltaint[0].operators.length, 2);
assertEq(fulltaint[0].operators[0].op, "substring");
assertEq(fulltaint[0].operators[0].param1, 5); //this should be the absolute start
assertEq(fulltaint[0].operators[0].param2, 10); // and end
// half
var halftaint=tainted.substring(3,8).taint;
assertEq(halftaint.length, 1);
assertEq(halftaint[0].begin, 5); //the taint starts at 5 even if we have some untainted string prepended
assertEq(halftaint[0].end, 8);
assertEq(halftaint[0].operators[0].param1, 3); //this should be the absolute start
assertEq(halftaint[0].operators[0].param2, 8); // and end

//substr
// substr's second parameter is relative, calculate absolute for taint
assertEq(JSON.stringify(tainted.substr(5,5).taint), JSON.stringify(fulltaint));
assertEq(JSON.stringify(tainted.substr(3,5).taint), JSON.stringify(halftaint));

//slice behaves like substring
assertEq(JSON.stringify(tainted.slice(5,10).taint), JSON.stringify(fulltaint));
assertEq(JSON.stringify(tainted.slice(3,8).taint), JSON.stringify(halftaint));


//check JIT operation
var add = tainted;
for(var i = 0; i < 100000; i++) {
	add = i + add;
}
assertEq(add.taint.length, 1);

if (typeof reportCompare === "function")
  reportCompare(true, true);