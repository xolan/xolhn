<template>
  <x-header :left-options="{ showBack: this.$route.path != '/', backText: 'Back' }">XolHN</x-header>
  <router-view></router-view>
</template>

<script>
import XHeader from 'vux-components/x-header';
import TopStories from './components/TopStories';

export default {
  components: {
    XHeader,
    TopStories,
  },
};
</script>

<style>
@import 'assets/vux.css';

#app {
  font-family: Verdana, Geneva, Tahoma, 'sans-serif';
}

html {
  overflow-y: scroll;
}

body, html {
  background: #292929!important;
  color: #EEE!important;
}

@media (min-width:1800px) {
  #app {
    padding: 0 30%;
  }
}

@media (max-width:1799px) and (min-width:1000px) {
  #app {
    padding: 0px 20%;
  }
}

@media (max-width:999px) and (min-width:500px) {
  #app {
    padding: 0px 10%;
  }
}

a {
  color: #ff6600;
}

a:visited {
  color: #70D8FF;
}

.vux-header {
  background: #ff6600!important;
}

.vux-header-left {
  color: #fff!important;
}

.vux-header-back {
  color: #FFF!important;
}

.vux-header-back::before {
  color: #FFF!important;
  border-color: #FFF!important;
  border-width: 2px 0 0 2px!important;
}

.weui_cells {
  background: #333!important;
}
</style>
