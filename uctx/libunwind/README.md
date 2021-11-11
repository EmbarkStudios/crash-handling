# libunwind

This directory contains the minimal sources we need from libunwind for (currently) just `getcontext`. The sources change extremely rarely, and it's easier to just copy sources manually than remove all of the sources (which is a majority of them) from the final packaged crate.
