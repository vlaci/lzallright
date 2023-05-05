#include "lzokay/lzokay.hpp"

namespace lzokay {

std::unique_ptr<DictBase> new_dict() {
  return std::unique_ptr<DictBase>(new Dict<>());
}

} // namespace lzokay
