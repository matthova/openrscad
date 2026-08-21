---
"openrscad-release-root": minor
---

Twisted and non-uniformly scaled extrusions now split each non-planar wall quad along its shorter diagonal, as OpenSCAD does, instead of picking one direction for the whole contour. This closes the last three known silent geometry differences: a twisted profile with a hole was 0.6% high in volume, one translated off the Z axis 1.3%, and a non-uniformly scaled curved profile 0.13% — each with an otherwise identical mesh.
