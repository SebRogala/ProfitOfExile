<template>

</template>

<script>
export default {
    name: 'FullShaperInvitation',
    data() {
        return {
        }
    },
    mounted() {
    },
    methods: {
        loadData() {
            this.$api.get('/rotation/full-shaper-invitation').then(res => {

            });
        }
    }
}
</script>
