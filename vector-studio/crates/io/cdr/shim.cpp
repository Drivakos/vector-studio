// Bridge from Rust to libcdr: reads a CorelDRAW (CDR) or Corel Presentation Exchange (CMX)
// file from memory and hands back one SVG document per page.

#include <climits>
#include <cstddef>
#include <librevenge/librevenge.h>
#include <librevenge-stream/librevenge-stream.h>
#include <libcdr/libcdr.h>

extern "C" {

typedef void (*vs_cdr_page)(void *ctx, const char *svg, size_t len);

// 0: read; 1: not a CDR or CMX file; 2: a CDR or CMX file that could not be read.
int vs_cdr_to_svg(const unsigned char *data, size_t len, vs_cdr_page page, void *ctx) {
  if (len > UINT_MAX)
    return 2;
  try {
    librevenge::RVNGStringStream input(data, static_cast<unsigned>(len));
    librevenge::RVNGStringVector pages;
    librevenge::RVNGSVGDrawingGenerator painter(pages, "");
    bool read;
    if (libcdr::CDRDocument::isSupported(&input))
      read = libcdr::CDRDocument::parse(&input, &painter);
    else if (libcdr::CMXDocument::isSupported(&input))
      read = libcdr::CMXDocument::parse(&input, &painter);
    else
      return 1;
    if (!read)
      return 2;
    for (unsigned i = 0; i < pages.size(); ++i)
      page(ctx, pages[i].cstr(), pages[i].size());
    return 0;
  } catch (...) {
    return 2;
  }
}

}
